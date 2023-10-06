#![allow(unused)]

use rand::Rng;

mod net;

fn generate_random_u8_vector(length: usize) -> Vec<u8> {
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
    use std::env;

    use crate::net::{client::Client, header::SendType, server::Server};

    use super::*;

    #[test]
    fn it_works() {
        env::set_var("RUST_LOG", "INFO");
        env_logger::init();

        let client_addr = "127.0.0.1:9091".parse().unwrap();
        let server_addr = "127.0.0.1:9090".parse().unwrap();

        let mut server = Server::start(server_addr, 64).unwrap();
        let mut client = Client::connect(client_addr, server_addr).unwrap();

        let mut read_buf = [0_u8; 1 << 16];
        
        let data = generate_random_u8_vector(1160);

        //to establish connection
        client.send(&data, SendType::Reliable).unwrap();

        let ev = server.read(&mut read_buf).unwrap();


        //server.send(client_addr, &data, SendType::Reliable).unwrap();
        loop {
            let n = client.read(&mut read_buf).unwrap();
        }
    }
}
