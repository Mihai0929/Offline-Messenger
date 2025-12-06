use project::ClientChat;
use project::protocol::Message;
use std::error::Error;
use std::io::Write;
use std::thread;

fn main() -> Result<(), Box<dyn Error>> {
    let client = ClientChat::connect("127.0.0.1:2024")?;
    println!("Conexiune realizata cu succes!");

    let (mut sender, receiver) = client.split();

    //Pregatesc thread-ul pentru ascultarea mesajelor
    thread::spawn(move || {
        while let Ok(msg) = receiver.recv() {
            match msg {
                Message::ToSend {
                    id,
                    from,
                    content,
                    time,
                } => {
                    println!("[#{}] [{}] {}: {}", id, time, from, content);
                }
                Message::HistoryData { content } => {
                    println!("Istoric");
                    for item in content {
                        println!("[{}] {}: {}", item.time, item.sender, item.content);
                    }
                }
                _ => {
                    println!("[server]: {:?}", msg)
                }
            }
            std::io::stdout().flush().ok();
        }
        //server-ul se deconecteaza
        std::process::exit(0);
    });

    println!("Commands: /login <user> <password>, /history <user>, <name>: <message>");

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

        if let Err(e) = sender.send_message(msg_to_send) {
            eprintln!("Eroare trimitere mesaj: {}", e);
            break;
        }
    }

    Ok(())
}
