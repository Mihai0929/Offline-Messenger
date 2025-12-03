use project::criptare::{ChannelSecure, RememberSecret};
use project::{send_data, receive_data};
use project::protocol::Message;
use std::error::Error;
use std::net::TcpStream;

fn main() -> Result<(), Box<dyn Error>> {
    let mut stream =TcpStream::connect("127.0.0.1:2024")?;

    let info = RememberSecret::new();
    let client_public_key = info.public_key.as_bytes().to_vec();

    let to_send = Message::ClientKey {public_key: client_public_key};
    let package = serde_json::to_vec(&to_send).expect("Eroare serializare");

    send_data(&mut stream, &package)?;

    //Asteptam ca server-ul sa trimita cheia
    let server_data = receive_data(&mut stream)?;

    let server_msg: Message = serde_json::from_slice(&server_data).expect("JSON Invalid de la server!");
    let server_public_key = match server_msg{
        Message::ServerKey { public_key } => public_key,
        _ => {println!("Protocol esuat!"); return Ok(());}
    };

    println!("Cheie client primita! Generam cheia comuna");

    let common_key = info.derive_key(server_public_key);
    let mut communication_channel = ChannelSecure::new(common_key);

    println!("Conexiune realizata cu succes!");
    Ok(())
}
