pub mod criptare;
pub mod protocol;

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::error::Error;

use crate::criptare::{ChannelSecure, RememberSecret};
use crate::protocol::Message;

pub fn send_data(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;

    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(data)?;

    Ok(())
}

pub fn receive_data(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_buff = [0u8; 4];
    stream.read_exact(&mut len_buff)?;

    let content_len = u32::from_be_bytes(len_buff) as usize;

    let mut buff = vec![0u8; content_len];
    stream.read_exact(&mut buff)?;

    Ok(buff)
}

pub struct ClientChat{
    stream: TcpStream,
    write_channel: ChannelSecure,
    pub receiver: mpsc::Receiver<Message>,
}

pub struct ClientSender {
    stream: TcpStream,
    write_channel: ChannelSecure,
}

impl ClientSender {
    pub fn send_message(&mut self, msg: Message) -> Result<(), Box<dyn Error>> {
        let bytes = serde_json::to_vec(&msg)?;
        
        let encrypted = self.write_channel.encrypt(&bytes)
            .map_err(|err| format!("Eroare criptare: {}", err))?;

        send_data(&mut self.stream, &encrypted)?;
        Ok(())
    }
}

impl ClientChat{
    pub fn connect(address: &str) -> Result<Self, Box<dyn Error>>{
        let mut stream = TcpStream::connect(address)?;

        let info = RememberSecret::new();
        let client_public_key = info.public_key.as_bytes().to_vec();
        let to_send = Message::ClientKey {
        public_key: client_public_key,
        };
        
        let package = serde_json::to_vec(&to_send).expect("Eroare serializare");
        send_data(&mut stream, &package)?;

        let server_data = receive_data(&mut stream)?;

        let server_msg: Message =
        serde_json::from_slice(&server_data).expect("JSON Invalid de la server!");
        let server_public_key = match server_msg {
            Message::ServerKey { public_key } => public_key,
            _ => return Err("Protocol esuat!".into())
        };
        
        println!("Cheie client primita! Generam cheia comuna");

        let common_key = info.derive_key(server_public_key);

        println!("Conexiune realizata cu succes!");

        let mut read_stream = stream.try_clone()?;

        let mut read_channel = ChannelSecure::new(common_key);
        let write_channel = ChannelSecure::new(common_key);

        let(tx, rx) = mpsc::channel::<Message>();
        
        //citim mesajul, il decriptam il parsam si il trimitem
        thread::spawn(move || {
            while let Ok(encrypted) = receive_data(&mut read_stream){
                let decrypted = match read_channel.decrypt(&encrypted){
                    Ok(data) => data,
                    Err(_) => continue
                };

                let msg = match serde_json::from_slice::<Message>(&decrypted){
                    Ok(msg) => msg,
                    Err(_) => continue
                };

                if tx.send(msg).is_err(){
                    break;
                }
            }
        });

        Ok(Self{
            stream,
            write_channel,
            receiver: rx,
        })
    }

    pub fn send_message(&mut self, msg: Message) -> Result<(), Box<dyn std::error::Error>>{
        let bytes = serde_json::to_vec(&msg)?;
        let encrypted = self.write_channel.encrypt(&bytes)
            .map_err(|err| format!("Eroare criptare: {}",err))?;

        send_data(&mut self.stream, &encrypted)?;
        Ok(())
    }

    pub fn try_receive_msg(&self) -> Result<Message, mpsc::TryRecvError>{
            self.receiver.try_recv()
    }

    pub fn split(self) -> (ClientSender, mpsc::Receiver<Message>){
        (ClientSender{
            stream: self.stream,
            write_channel: self.write_channel,
        },
        self.receiver
        )
    }
}
