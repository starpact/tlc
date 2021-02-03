use std::io::Read;
use std::sync::mpsc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel();
    tlc::init(rx)?;

    let mut stdin = std::io::stdin();
    let mut buf = [0; 3];

    loop {
        stdin.read(&mut buf)?;
        let input = buf[0] - 48;

        println!("start working...");
        tx.send(input)?;
    }
}
