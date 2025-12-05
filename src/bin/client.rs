use project::criptare::{ChannelSecure, RememberSecret};
use project::protocol::Message;
use project::{receive_data, send_data};
use std::error::Error;
use std::io::Write;
use std::net::TcpStream;
use std::thread;

fn main() -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:2024")?;

    let info = RememberSecret::new();
    let client_public_key = info.public_key.as_bytes().to_vec();

    let to_send = Message::ClientKey {
        public_key: client_public_key,
    };
    let package = serde_json::to_vec(&to_send).expect("Eroare serializare");

    send_data(&mut stream, &package)?;

    //Asteptam ca server-ul sa trimita cheia
    let server_data = receive_data(&mut stream)?;

    let server_msg: Message =
        serde_json::from_slice(&server_data).expect("JSON Invalid de la server!");
    let server_public_key = match server_msg {
        Message::ServerKey { public_key } => public_key,
        _ => {
            println!("Protocol esuat!");
            return Ok(());
        }
    };

    println!("Cheie client primita! Generam cheia comuna");

    let common_key = info.derive_key(server_public_key);
    let communication_channel = ChannelSecure::new(common_key);

    println!("Conexiune realizata cu succes!");

    let mut read_stream = stream.try_clone()?;

    let mut read_channel = ChannelSecure::new(common_key);
    let mut write_channel = communication_channel;

    //Pregatesc thread-ul pentru ascultarea mesajelor
    thread::spawn(move || {
        loop {
            //Primesc informatiile criptate
            let encrypted = match receive_data(&mut read_stream) {
                Ok(data) => data,
                Err(_) => {
                    eprintln!("Server-ul a fost inchis");
                    break;
                }
            };

            //Decriptam continutul
            let decrypted = match read_channel.decrypt(&encrypted) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Eroare: mesaj corupt sau cheie gresita: {e}");
                    continue;
                }
            };

            //Parsam datele si printam
            let msg: Message = match serde_json::from_slice(&decrypted) {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("Eroare: JSON invalid: {e}");
                    continue;
                }
            };

            match msg {
                Message::ToSend {
                    id,
                    from,
                    content,
                    time,
                } => {
                    println!("ID: {} | [{}] {}: {}", id, time, from, content);
                }
                Message::HistoryData { content } => {
                    println!("\r Istoric");
                    for message in content {
                        println!("[{}] {}: {}", message.time, message.sender, message.content);
                    }
                }
                _ => {
                    println!("\r[server]: {:?}", msg);
                }
            }

            //Curat stream-ul
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    });

    println!("Commands: login <user> <password>, history <user>, <name>: <message>");

    loop {
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let content = input.trim().to_string();

        if content.is_empty() {
            continue;
        }
        if content == "/quit" {
            break;
        }

        let msg_to_send = if content.starts_with("/login") {
            let tokens: Vec<&str> = content.split_whitespace().collect();
            if tokens.len() == 3 {
                Message::Login {
                    username: tokens[1].to_string(),
                    password: tokens[2].to_string(),
                }
            } else {
                println!("Wrong usage!Format: /login <user> <password>");
                continue;
            }
        } else if content.starts_with("/history") {
            let tokens: Vec<&str> = content.split_whitespace().collect();
            if tokens.len() == 2 {
                Message::HistoryInfo {
                    user: tokens[1].to_string(),
                }
            } else {
                println!("Wrong usage!Format: /history <user>");
                continue;
            }
        } else if let Some((dest, text)) = content.split_once(':') {
            Message::Text {
                to: dest.trim().to_string(),
                content: text.to_string(),
                reply_id: None,
            }
        } else {
            println!("Invalid format!Use: recipient: message");
            continue;
        };

        let json_content = serde_json::to_vec(&msg_to_send).expect("Eroare serializare");

        let encrypted_content = match write_channel.encrypt(&json_content) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Eroare criptare: {}", e);
                continue;
            }
        };

        send_data(&mut stream, &encrypted_content)?;
    }

    Ok(())
}
