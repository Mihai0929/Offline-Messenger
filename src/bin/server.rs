use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

fn client_handle(stream: TcpStream, clients: Arc<Mutex<HashMap<String, TcpStream>>>){
    println!("Client nou conectat!");
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
    println!("Asteptam clienti la conectarea serverului...");

    //Acceptam conexiunile si le procesam in ordine
    for stream_info in stream.incoming(){
        match stream_info{
        Ok(stream) => {
        //Las o copie la mp pentru a procesa utilizatorii fara pierderi de informatii
        let mp_copy = connected_clients.clone();

        //Pregatim thread-uri pentru toti clientii   
        thread::spawn(move || {
            client_handle(stream, mp_copy);
        });
        }

        Err(e) => {
            println!("Eroare la conectarea cu server-ul: {e}");
        }
        }
    }
    Ok(())
}
