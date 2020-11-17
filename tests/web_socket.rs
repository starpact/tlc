#[cfg(test)]

mod web_socket {
    use tlc::web_socket::*;

    #[test]
    fn test_mio_server() {
        server::serve().unwrap();
    }
}
