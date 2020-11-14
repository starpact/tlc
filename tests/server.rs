#[cfg(test)]

mod server {
    use std::io::prelude::*;
    use std::fs;
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn test_server() {
        let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            println!("come!");
            handle_connection(stream);
        }
    }

    fn handle_connection(mut stream: TcpStream) {
        let mut buffer = [0; 512];
        stream.read(&mut buffer).unwrap();

        let content = fs::read_to_string("./resource/hello.html").unwrap();

        let response = format!("HTTP/1.1 200 OK\r\n\r\n{}", content);

        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    }
}
