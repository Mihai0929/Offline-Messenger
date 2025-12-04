use project::criptare::{ChannelSecure, RememberSecret};
use project::protocol::{Message, MessageHistoryInfo};
use project::{receive_data, send_data};
use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::params;

fn client_handle(mut stream: TcpStream, clients: Arc<Mutex<HashMap<String, TcpStream>>>) {
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
    let mut communication_channel = ChannelSecure::new(common_key);

    println!("Conexiune realizata cu succes!");

    let mut curr_user: Option<String> = None;

    loop {
        //Citim pachetul criptat
        let encrypted_package = match receive_data(&mut stream) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Eroare receive_data: {e}");
                return;
            }
        };

        //Decriptam continutul
        let decrypted_package = match communication_channel.decrypt(&encrypted_package) {
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

        match msg{
            Message::Login { username, password } => {
                let mut hash_func = Sha256::new();
                hash_func.update(password.as_bytes());
                let res = hash_func.finalize();
                
                let password_hash = format!("{:x}", res);
                
                let cnt: i64 = match conn.query_row("SELECT count(*) FROM users WHERE username = ?1", params![username], |row| row.get(0)){
                    Ok(count) => count,
                    Err(_) => {
                        0
                    }
                };

                //Inregistram utilizatorul
                let ok = if cnt == 0{
                    conn.execute("INSERT INTO users (username, password) VALUES (?1, ?2)", params![username, password_hash]).is_ok()
                }
                else{
                    //In caz ca nu am parola compar cu un string empty sa fie eroare
                    let pass: String = match conn.query_row("SELECT password FROM users WHERE username = ?1", params![username], |row| row.get(0)){
                        Ok(pass) => pass,
                        Err(_) => String::new(),
                    };
                    pass == password_hash
                };

                if ok{
                    println!("Login realizat cu succes!");

                    curr_user = Some(username.clone());

                    if let Ok(mut map) = clients.lock(){
                        if let Ok(mut stream_copy) = stream.try_clone(){
                            map.insert(username.clone(), stream_copy);
                        }
                    }

                    //Trimitem mesajele offline la user-ul logged
                    let mut statement = match conn.prepare("SELECT sender, content, time FROM messages WHERE delivered = 0 AND receiver = ?1"){
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Eroare la preluarea informatiilor: {}", e);
                            return ;
                        }
                    };

                    let mut rows_iterator = match statement.query(params![username]){
                        Ok(rows) => rows,
                        Err(e) => {eprintln!("Eroare query: {}", e); return;}
                    };

                    //Iteram prin map
                    while let Ok(Some(row)) = rows_iterator.next(){
                        
                        let sender:String = match row.get(0){
                            Ok(s) => s,
                            Err(_) => {continue;}
                        };

                        let content:String = match row.get(1){
                            Ok(c) => c,
                            Err(_) => {continue;}
                        };

                        let time:u64 = match row.get(2){
                            Ok(t) => t,
                            Err(_) => {continue;}
                        };

                        let package = Message::ToSend { from: sender, content, time };
                        let bytes_package = match serde_json::to_vec(&package){
                            Ok(content) => content,
                            Err(e) => {eprintln!("Eroare serializare mesaje de trimis: {}", e); break;}
                        };

                        if let Ok(encrypted_data) = communication_channel.encrypt(&bytes_package){
                            send_data(&mut stream, &encrypted_data);
                        }
                    }
                }
            },
            
            _ => {
                println!("todo!");
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
    let connected_clients = Arc::new(Mutex::new(HashMap::<String, TcpStream>::new()));

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
