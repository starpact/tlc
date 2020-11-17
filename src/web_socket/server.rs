use std::error::Error;

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

pub fn serve() -> Result<(), Box<dyn Error>> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    let addr = "127.0.0.1:2333".parse()?;

    let mut server = TcpListener::bind(addr)?;
    poll.registry()
        .register(&mut server, SERVER, Interest::READABLE)?;

    let mut client = TcpStream::connect(addr)?;
    poll.registry()
        .register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE)?;

    loop {
        println!("poll");
        poll.poll(&mut events, None)?;
        
        for event in events.iter() {
            match event.token() {
                SERVER => {
                    println!("server");
                    let connection = server.accept()?;
                    drop(connection);
                }
                CLIENT => {
                    println!("client");
                    if event.is_writable() {}
                    if event.is_readable() {}
                }
                _ => println!("sss"),
            }
        }
    }
}
