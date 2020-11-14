use std::fs;
use std::io;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::thread;

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;

    println!("[Server]: waiting...");
    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(|| {
            handle_connection(stream).unwrap();
            println!("[Server]: waiting...");
        });
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 512];
    stream.read(&mut buffer)?;

    let get_request = b"GET / HTTP/1.1\r\n";
    let contents = if buffer.starts_with(get_request) {
        println!("Get /");
        fs::read_to_string("./resource/hello.html")?
    } else {
        format!(
            "{}",
            String::from_utf8_lossy(&buffer).lines().nth(0).unwrap()
        )
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
        contents.len(),
        contents
    );

    stream.write(response.as_bytes())?;
    stream.flush()?;

    Ok(())
}
