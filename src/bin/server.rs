use project::criptare::{ChannelSecure, RememberSecret};
use project::protocol::{Message, MessageHistoryInfo};
use project::{receive_data, send_data};
use rusqlite::params;
use rusqlite::{Connection, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn client_handle(mut stream: TcpStream, clients: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>) {
    println!("Client nou conectat!");

    //Pentru fiecare client in parte realizez conexiunea la baze de date
    let conn = Connection::open("info.db").expect("Eroare la deschiderea bazei de date");

    //Realizam etapa de criptare
    let info = RememberSecret::new();
    let server_public_key = info.public_key.as_bytes().to_vec();

    //Citim cheia publica de la client
    let data = match receive_data(&mut stream) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Eroare la preluarea informatiilor: {e}");
            return;
        }
    };

    let response = Message::ServerKey {
        public_key: server_public_key,
    };
    let response_bytes = serde_json::to_vec(&response).expect("Eroare serializare!");

    if let Err(e) = send_data(&mut stream, &response_bytes) {
        eprintln!("Eroare la trimiterea continutului: {e}");
        return;
    }

    let client_msg: Message = serde_json::from_slice(&data).expect("JSON Invalid de la client!");

    let client_public_key = match client_msg {
        Message::ClientKey { public_key } => public_key,
        _ => {
            println!("Protocol esuat!");
            return;
        }
    };

    println!("Cheie client primita! Generam cheia comuna");

    //Realizam conexiunea
    let common_key = info.derive_key(client_public_key);

    //Am nevoie de 2 canale de comunicare, unul de citire unde decriptez continutul primit
    //unul de scriere, criptez continutul si tirmit
    let mut read_channel = ChannelSecure::new(common_key);
    let mut write_channel = ChannelSecure::new(common_key);
    println!("Conexiune realizata cu succes!");

    let (tx, rx) = mpsc::channel::<Message>();

    let mut writer_thread = stream.try_clone().expect("Eroare stream clone");

    //Pregatim thread-ul pentru ascultare
    thread::spawn(move || {
        while let Ok(msg) = rx.recv(){
            if let Ok(bytes) = serde_json::to_vec(&msg) && let Ok(crypted) = write_channel.encrypt(&bytes) && send_data(&mut writer_thread, &crypted).is_err(){
                break;
            }
        }
    });

    let mut curr_user: Option<String> = None;

    loop {
        //Citim pachetul criptat
        let encrypted_package = match receive_data(&mut stream) {
            Ok(bytes) => bytes,
            Err(_) => {
                if let Some(user) = curr_user && let Ok(mut mp) = clients.lock(){
                        mp.remove(&user);
                }
                break;
            }
        };

        //Decriptam continutul
        let decrypted_package = match read_channel.decrypt(&encrypted_package) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Eroare decrypt: {e}");
                return;
            }
        };

        let msg: Message = match serde_json::from_slice(&decrypted_package) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("JSON invalid: {e}");
                return;
            }
        };

        match msg {
            Message::Login { username, password } => {
                let mut hash_func = Sha256::new();
                hash_func.update(password.as_bytes());
                let res = hash_func.finalize();

                let password_hash = format!("{:x}", res);

                let cnt: i64 = conn
                    .query_row(
                        "SELECT count(*) FROM users WHERE username = ?1",
                        params![username],
                        |row| row.get(0),
                    )
                    .unwrap_or_default();

                //Inregistram utilizatorul
                let ok = if cnt == 0 {
                    conn.execute(
                        "INSERT INTO users (username, password) VALUES (?1, ?2)",
                        params![username, password_hash],
                    )
                    .is_ok()
                } else {
                    //In caz ca nu am parola compar cu un string empty sa fie eroare
                    let pass: String = conn
                        .query_row(
                            "SELECT password FROM users WHERE username = ?1",
                            params![username],
                            |row| row.get(0),
                        )
                        .unwrap_or_default();
                    pass == password_hash
                };

                if ok {
                    println!("Login realizat cu succes!");

                    curr_user = Some(username.clone());

                    //retinem sender-ul in map
                    if let Ok(mut map) = clients.lock(){
                        map.insert(username.clone(), tx.clone());
                    }

                    //Trimitem mesajele offline la user-ul logged
                    let mut statement = match conn.prepare("SELECT sender, content, time, id FROM messages WHERE delivered = 0 AND receiver = ?1"){
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Eroare la preluarea informatiilor: {}", e);
                            return ;
                        }
                    };

                    let mut rows_iterator = match statement.query(params![username]) {
                        Ok(rows) => rows,
                        Err(e) => {
                            eprintln!("Eroare query: {}", e);
                            return;
                        }
                    };

                    //Iteram prin map
                    while let Ok(Some(row)) = rows_iterator.next() {
                        let sender: String = match row.get(0) {
                            Ok(s) => s,
                            Err(_) => {
                                continue;
                            }
                        };

                        let content: String = match row.get(1) {
                            Ok(c) => c,
                            Err(_) => {
                                continue;
                            }
                        };

                        let time: u64 = match row.get(2) {
                            Ok(t) => t,
                            Err(_) => {
                                continue;
                            }
                        };
                        let curr_id: u64 = match row.get(3) {
                            Ok(i) => i,
                            Err(_) => {
                                continue;
                            }
                        };

                        let msg = Message::ToSend { id: curr_id, from: sender, content, time };

                        tx.send(msg).ok();
                    }

                    //Marcam mesajele ca delivered
                    conn.execute(
                        "UPDATE messages SET delivered = 1 WHERE delivered = 0 and receiver = ?1",
                        params![username],
                    )
                    .ok();
                }
                else{
                    println!("Login esuat! parola incorecta");
                }
            }
            Message::Text {
                to,
                content,
                reply_id,
            } => {
                if let Some(ref sender) = curr_user {
                    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
                        Ok(info) => info.as_secs(),
                        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
                    };

                    match conn.execute("INSERT INTO messages (sender, receiver, id, content, time, delivered) VALUES (?1, ?2, ?3, ?4, ?5, 0)", params![sender, to, reply_id, content, time]){
                        Ok(_) => println!("Mesaj salvat cu succes({} -> {})", sender, to),
                        Err(e) => eprintln!("Eroare la trimiterea mesajului: {}", e),
                    }

                    let mut delivered = false;
                    if let Ok(map) = clients.lock(){
                        //Verificam daca user-ul este pe canal si ii trimitem mesajul
                        if let Some(recipient_tx) = map.get(&to){
                            let msg = Message::ToSend { id: 0, from: sender.clone(), content: content.clone(), time };
                            if recipient_tx.send(msg).is_ok(){
                                delivered = true;
                            }
                        }
                    }

                    //daca mesajul s-a trimis dam update la baza de date
                    if delivered{
                        conn.execute("UPDATE messages SET delivered = 1 WHERE sender = ?1 and receiver = ?2 and time = ?3", params![sender, to, time]).ok();
                        println!("Mesaj livrat cu succes catre {}", to);
                    }
                    else{
                        println!("Mesaj offline salvat pentru {}", to);
                    }
                } else {
                    println!("Trimiterea mesajelor necesita autentificare!");
                }
            }
            Message::HistoryInfo { user } => {
                if let Some(ref conn_user) = curr_user {
                    //Realizez interogare pentru aflarea mesajelor transmise intre                         2 utilizatori
                    let mut statement = match conn.prepare("SELECT id, sender, content, time, delivered FROM messages
                                                                    WHERE (sender = ?1 AND receiver = ?2) OR (sender = ?2 AND receiver = ?1) ORDER BY time ASC")
                    {
                        Ok(s) => s,
                        Err(e) => {eprintln!("Eroare baza de date preluare informatii istoric: {}", e); continue;}
                    };

                    let mut rows_iterator = match statement.query(params![conn_user, user]) {
                        Ok(rows) => rows,
                        Err(e) => {
                            eprintln!("Eroare query: {e}");
                            return;
                        }
                    };

                    let mut history_vector = Vec::new();

                    //Iteram prin map
                    while let Ok(Some(row)) = rows_iterator.next() {
                        let message_id: u64 = match row.get(0) {
                            Ok(id) => id,
                            Err(_) => {
                                continue;
                            }
                        };

                        let sender: String = match row.get(1) {
                            Ok(s) => s,
                            Err(_) => {
                                continue;
                            }
                        };

                        let content: String = match row.get(2) {
                            Ok(c) => c,
                            Err(_) => {
                                continue;
                            }
                        };

                        let time: i64 = match row.get(3) {
                            Ok(t) => t,
                            Err(_) => {
                                continue;
                            }
                        };

                        let delivered_success: i64 = match row.get(4){
                            Ok(d) => d,
                            Err(_) => {
                                continue;
                            }
                        };

                        let package = MessageHistoryInfo {
                            message_id,
                            sender,
                            content,
                            time,
                            delivered: delivered_success == 1,
                        };

                        history_vector.push(package);
                    }

                    let package = Message::HistoryData { content: history_vector };
                    tx.send(package).ok();

                    println!("Istoric trimis catre {}", conn_user);
                } else {
                    println!("Aceasta comanda necesita autentificare!");
                }
            }
            _ => {
                println!("Comanda invalida");
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::open("info.db")?;

    //Realizez 2 baze de date pentru userii conectati dar si informatiile mesajelor transmise

    conn.execute(
        "CREATE TABLE IF NOT EXISTS
                        users(username TEXT PRIMARY KEY,
                              password TEXT NOT NULL)",
        (),
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS
                        messages(sender TEXT NOT NULL,
                                 receiver TEXT NOT NULL,
                                 id INTEGER PRIMARY KEY,
                                 content TEXT NOT NULL,
                                 time INTEGER NOT NULL,
                                 delivered INTEGER DEFAULT 0)",
        (),
    )?;

    //Retin intr-un map pentru fiecare utilizator inregistrat user-ul si socket-ul
    let connected_clients = Arc::new(Mutex::new(HashMap::<String, mpsc::Sender<Message>>::new()));

    //Setam portul 2024 pentru server
    let stream = TcpListener::bind("127.0.0.1:2024")?;
    println!("[server]Asteptam clienti la conectarea serverului...");

    //Acceptam conexiunile si le procesam in ordine
    for stream_info in stream.incoming() {
        match stream_info {
            Ok(stream) => {
                //Las o copie la mp pentru a procesa utilizatorii fara pierderi de informatii
                let mp_copy = connected_clients.clone();

                //Pregatim thread-uri pentru toti clientii
                thread::spawn(move || {
                    client_handle(stream, mp_copy);
                });
            }

            Err(e) => {
                println!("[server]Eroare la conectarea cu server-ul: {e}");
            }
        }
    }
    Ok(())
}
