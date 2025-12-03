pub mod criptare;
pub mod protocol;

use std::net::TcpStream;
use std::io::{self, Read, Write};

pub fn send_data(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;

    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(data)?;

    Ok(())
}

pub fn receive_data(stream: &mut TcpStream) -> io::Result<Vec<u8>>{
    let mut len_buff = [0u8; 4];
    stream.read_exact(&mut len_buff)?;

    let content_len = u32::from_be_bytes(len_buff) as usize;

    let mut buff = vec![0u8; content_len];
    stream.read_exact(&mut buff)?;

    Ok(buff)
}
