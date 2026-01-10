use project::criptare::{ChannelSecure, RememberSecret};
use project::protocol::{Message, MessageHistoryInfo};
use project::{log_error, receive_data, send_data};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn perform_handshake(
    stream: &mut TcpStream,
) -> Result<(ChannelSecure, ChannelSecure), Box<dyn Error>> {
    let info = RememberSecret::new();
    let server_public_key = info.public_key.as_bytes().to_vec();

    //citire cheie client
    let data = receive_data(stream)?;

    //trimitere cheie server
    let response = Message::ServerKey {
        public_key: server_public_key,
    };
    let response_bytes = serde_json::to_vec(&response)?;

    send_data(stream, &response_bytes)?;

    let client_msg: Message = serde_json::from_slice(&data)?;

    let client_public_key = match client_msg {
        Message::ClientKey { public_key } => public_key,
        _ => return Err("Protocol esuat: ClientKey expected".into()),
    };

    println!("Cheie client primita! Generam cheia comuna");
    let common_key = info.derive_key(client_public_key);

    Ok((
        ChannelSecure::new(common_key),
        ChannelSecure::new(common_key),
    ))
}

fn process_login(
    conn: &Connection,
    username: &str,
    password: &str,
) -> Result<bool, Box<dyn Error>> {
    let mut hash_func = Sha256::new();
    hash_func.update(password.as_bytes());
    let res = hash_func.finalize();
    let password_hash = format!("{:x}", res);

    let cnt: i64 = conn.query_row(
        "SELECT count(*) FROM users WHERE username = ?1",
        params![username],
        |row| row.get(0),
    )?;

    if cnt == 0 {
        conn.execute(
            "INSERT INTO users (username, password) VALUES (?1, ?2)",
            params![username, password_hash],
        )?;
        Ok(true)
    } else {
        let pass: String = conn.query_row(
            "SELECT password FROM users WHERE username = ?1",
            params![username],
            |row| row.get(0),
        )?;
        Ok(pass == password_hash)
    }
}

fn send_offline_messages(
    conn: &Connection,
    username: &str,
    tx: &mpsc::Sender<Message>,
) -> Result<(), Box<dyn Error>> {
    let mut statement = conn.prepare(
        "SELECT sender, content, time, reply_id, id FROM messages
         WHERE delivered = 0 AND receiver = ?1",
    )?;

    let mut rows_iterator = statement.query(params![username])?;

    while let Some(row) = rows_iterator.next()? {
        let sender: String = row.get(0)?;
        let content: String = row.get(1)?;
        let time: i64 = row.get(2)?;
        let reply_id: Option<u64> = row.get(3)?;
        let curr_id: u64 = row.get(4)?;

        let msg = Message::ToSend {
            id: curr_id,
            from: sender,
            content,
            time,
            reply_id,
        };

        if let Err(e) = tx.send(msg) {
            log_error("Trimitere mesaj cand e canalul de comunicare inchis", e);
        }
    }

    conn.execute(
        "UPDATE messages SET delivered = 1 WHERE delivered = 0 and receiver = ?1",
        params![username],
    )?;

    Ok(())
}

fn process_text_message(
    conn: &Connection,
    clients: &Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
    tx: &mpsc::Sender<Message>,
    sender: &str,
    to: String,
    content: String,
    reply_id: Option<u64>,
) -> Result<(), Box<dyn Error>> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("SystemTime Error: {}", e))?
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO messages (sender, receiver, content, time, delivered, reply_id)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        params![sender, to, content, time, reply_id],
    )?;

    let message_id = conn.last_insert_rowid() as u64;
    println!("Mesaj salvat cu succes({} -> {})", sender, to);

    let echo_msg = Message::ToSend {
        id: message_id,
        from: sender.to_string(),
        content: content.clone(),
        time,
        reply_id,
    };
    tx.send(echo_msg)?;

    let mut delivered = false;
    {
        if let Ok(map) = clients.lock()
            && let Some(recipient_tx) = map.get(&to)
        {
            let msg = Message::ToSend {
                id: message_id,
                from: sender.to_string(),
                content,
                time,
                reply_id,
            };
            if recipient_tx.send(msg).is_ok() {
                delivered = true;
            }
        }
    }

    if delivered {
        conn.execute(
            "UPDATE messages SET delivered = 1 WHERE sender = ?1 and receiver = ?2 and time = ?3",
            params![sender, to, time],
        )?;
        println!("Mesaj livrat cu succes catre {}", to);
    } else {
        println!("Mesaj offline salvat pentru {}", to);
    }

    Ok(())
}

fn process_history_request(
    conn: &Connection,
    tx: &mpsc::Sender<Message>,
    curr_user: &str,
    other_user: String,
) -> Result<(), Box<dyn Error>> {
    let mut statement = conn.prepare(
        "SELECT id, sender, content, time, delivered, reply_id FROM messages
         WHERE (sender = ?1 AND receiver = ?2) OR (sender = ?2 AND receiver = ?1) ORDER BY time ASC",
    )?;

    let mut rows_iterator = statement.query(params![curr_user, other_user])?;
    let mut history_vector = Vec::new();

    while let Some(row) = rows_iterator.next()? {
        let message_id: u64 = row.get(0)?;
        let sender: String = row.get(1)?;
        let content: String = row.get(2)?;
        let time: i64 = row.get(3)?;
        let delivered_int: i64 = row.get(4)?;
        let reply_id: Option<u64> = row.get(5)?;

        let package = MessageHistoryInfo {
            message_id,
            sender,
            content,
            time,
            delivered: delivered_int == 1,
            reply_id,
        };

        history_vector.push(package);
    }

    let package = Message::HistoryData {
        content: history_vector,
    };
    tx.send(package)?;

    println!("Istoric trimis catre {}", curr_user);
    Ok(())
}

fn client_handle(
    mut stream: TcpStream,
    clients: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
) {
    println!("Client nou conectat!");

    let conn = match Connection::open("info.db") {
        Ok(c) => c,
        Err(e) => {
            log_error("Database Connection", e);
            return;
        }
    };

    //realizam handshake-ul
    let (mut read_channel, mut write_channel) = match perform_handshake(&mut stream) {
        Ok(channels) => channels,
        Err(e) => {
            log_error("Handshake", e);
            return;
        }
    };

    println!("Conexiune realizata cu succes!");

    let (tx, rx) = mpsc::channel::<Message>();

    let mut writer_thread = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            log_error("Stream clone", e);
            return;
        }
    };

    thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            let bytes = match serde_json::to_vec(&msg) {
                Ok(b) => b,
                Err(e) => {
                    log_error("Serializare thread", e);
                    break;
                }
            };
            let crypted = match write_channel.encrypt(&bytes) {
                Ok(c) => c,
                Err(e) => {
                    log_error("Criptare thread", e);
                    break;
                }
            };
            if let Err(e) = send_data(&mut writer_thread, &crypted) {
                log_error("Socket thread send", e);
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
                if let Some(user) = curr_user
                    && let Ok(mut mp) = clients.lock()
                {
                    mp.remove(&user);
                }
                break;
            }
        };

        //Decriptam continutul
        let decrypted_package = match read_channel.decrypt(&encrypted_package) {
            Ok(bytes) => bytes,
            Err(e) => {
                log_error("Decryption", e);
                continue;
            }
        };

        let msg: Message = match serde_json::from_slice(&decrypted_package) {
            Ok(msg) => msg,
            Err(e) => {
                log_error("Deserializare JSON", e);
                continue;
            }
        };

        let processing_result: Result<(), Box<dyn Error>> = match msg {
            Message::Login { username, password } => {
                match process_login(&conn, &username, &password) {
                    Ok(true) => {
                        println!("Login realizat cu succes!");
                        curr_user = Some(username.clone());

                        if let Ok(mut map) = clients.lock() {
                            map.insert(username.clone(), tx.clone());
                        }
                        if let Err(e) = send_offline_messages(&conn, &username, &tx) {
                            log_error("Offline Messages", e);
                        }
                    }
                    Ok(false) => {
                        log_error(
                            "Auth Failed",
                            format!("Incorrect password for {}", username),
                        );
                    }
                    Err(e) => {
                        log_error("Login Process", e);
                    }
                }
                Ok(())
            }

            Message::Text {
                to,
                content,
                reply_id,
            } => {
                if let Some(ref sender) = curr_user {
                    process_text_message(&conn, &clients, &tx, sender, to, content, reply_id)
                } else {
                    log_error("Permisiuni incalcate", "Incercare trimitere mesaj fara login");
                    Ok(())
                }
            }

            Message::HistoryInfo { user } => {
                if let Some(ref conn_user) = curr_user {
                    process_history_request(&conn, &tx, conn_user, user)
                } else {
                    log_error("Permisiuni incalcate", "Cererea istoricului fara login");
                    Ok(())
                }
            }
            _ => {
                log_error("Protocol", "Comanda inexistenta");
                Ok(())
            }
        };

        if let Err(e) = processing_result {
            log_error("Procesare mesaje", e);
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
                                 reply_id INTEGER,
                                 content TEXT NOT NULL,
                                 time INTEGER NOT NULL,
                                 delivered INTEGER DEFAULT 0)",
        (),
    )?;

    conn.execute("ALTER TABLE messages ADD COLUMN reply_id INTEGER", ())
        .ok();

    //Retin intr-un map pentru fiecare utilizator inregistrat user-ul si socket-ul
    let connected_clients = Arc::new(Mutex::new(HashMap::<String, mpsc::Sender<Message>>::new()));

    let stream = TcpListener::bind("127.0.0.1:2024")?;
    println!("[server]Asteptam clienti la conectarea serverului...");

    for stream_info in stream.incoming() {
        match stream_info {
            Ok(stream) => {
                let mp_copy = connected_clients.clone();
                thread::spawn(move || {
                    client_handle(stream, mp_copy);
                });
            }

            Err(e) => {
                log_error("Eroare conectare cu server-ul", e);
            }
        }
    }
    Ok(())
}
