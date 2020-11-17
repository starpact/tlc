use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:2333")?;

    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(move || {
            handle_connection(stream).unwrap_or(());
        });
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = [0; 1024];
    stream.read(&mut buf)?;
    println!("{}", String::from_utf8_lossy(&buf).lines().nth(0).unwrap());
    stream.write("HTTP/1.1 200 OK\r\n\r\n".as_bytes())?;

    let simple_get_head = b"GET / HTTP/1.1\r\n";
    if buf.starts_with(simple_get_head) {
        let file = File::open("./config/config_large.json")?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        stream.write(&buf[..])?;
    }

    stream.flush()?;

    Ok(())
}
