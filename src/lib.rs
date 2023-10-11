#![allow(unused)]

use net::Bytes;
use rand::Rng;

mod net;

fn generate_random_u8_vector(length: usize) -> Bytes {
    let mut rng = rand::thread_rng();
    let mut result = Vec::with_capacity(length);

    for _ in 0..length {
        let random_u8: u8 = rng.gen();
        result.push(random_u8);
    }

    result
}

#[cfg(test)]
mod tests {
    use std::{env, thread, time::Duration};

    use crate::net::{
        client::Client,
        header::SendType,
        server::{Server, ServerEvent},
    };

    use super::*;

    #[test]
    fn it_works() {
        env::set_var("RUST_LOG", "DEBUG");
        env_logger::init();

        let client_addr = "127.0.0.1:9091".parse().unwrap();
        let server_addr = "127.0.0.1:9090".parse().unwrap();

        let mut server = Server::start(server_addr, 64).unwrap();
        let mut client = Client::connect(client_addr, server_addr).unwrap();

        let mut read_buf = [0_u8; 1 << 16];

        assert!(
            matches!(
                server.read(&mut read_buf).unwrap(),
                ServerEvent::NewConnection(1)
            ),
            "expected new connection"
        );

        let data = generate_random_u8_vector(1160);

        assert!(client.send(&data, SendType::Reliable).is_ok());
        thread::sleep(Duration::from_secs(120));
        /*match server.read(&mut read_buf).unwrap() {
            ServerEvent::Receive(1, d) => assert_eq!(data, d),
            _ => panic!(""),
        }*/
    }
}
