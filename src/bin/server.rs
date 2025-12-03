use project::criptare::{ChannelSecure, RememberSecret};
use project::protocol::Message;
use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

fn send_data(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;

    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(data)?;

    Ok(())
}

fn receive_data(stream: &mut TcpStream) -> io::Result<Vec<u8>>{
    let mut len_buff = [0u8; 4];
    stream.read_exact(&mut len_buff)?;

    let content_len = u32::from_be_bytes(len_buff) as usize;

    let mut buff = vec![0u8; content_len];
    stream.read_exact(&mut buff)?;

    Ok(buff)
}

fn client_handle(mut stream: TcpStream, clients: Arc<Mutex<HashMap<String, TcpStream>>>) {
    println!("Client nou conectat!");

    //Pentru fiecare client in parte realizez conexiunea la baze de date
    let conn = Connection::open("info.db").expect("Eroare la deschiderea bazei de date");

    //Realizam etapa de criptare
    let info = RememberSecret::new();
    let client_public_key = info.public_key.as_bytes().to_vec();

    //Citim cheia publica de la client
    let data = match receive_data(&mut stream){
        Ok(content) => content,
        Err(e) => {eprintln!("Eroare la preluarea informatiilor: {e}"); return;} 
    };

    let client_msg: Message = serde_json::from_slice(&data).expect("JSON Invalid!");

    let client_public_key = match client_msg{
        Message::ClientKey { public_key } => public_key,
        _ => {println!("Protocol esuat!"); return;} 
    };

    //Realizam conexiunea
    let common_key = info.derive_key(client_public_key);
    let mut communication_channel = ChannelSecure::new(common_key);

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
