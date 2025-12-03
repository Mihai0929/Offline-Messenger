use std::error::Error;
use std::net::TcpStream;

fn main() -> Result<(), Box<dyn Error>> {
    match TcpStream::connect("127.0.0.1:2024") {
        Ok(stream) => {
            println!(
                "[client] Sunt conectat la server cu adresa {:?}",
                stream.local_addr()?
            );
        }
        Err(e) => {
            println!("[client] Eroare la conectare! {}", e);
        }
    }
    Ok(())
}
