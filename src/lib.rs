#![allow(unused)]

use net::Bytes;
use rand::Rng;

mod net;

#[cfg(test)]
mod tests {
    use std::{
        env,
        thread::{self, sleep},
        time::Duration,
    };

    use crate::net::{Client, SendType, Server, ServerEvent, FRAGMENT_SIZE, MAX_FRAGMENT_SIZE};

    use super::*;

    #[test]
    fn it_works() {
        env::set_var("RUST_LOG", "DEBUG");
        env_logger::init();

        let client_addr = "127.0.0.1:9091".parse().unwrap();
        let server_addr = "127.0.0.1:9090".parse().unwrap();

        let mut server = Server::start(server_addr, 64).unwrap();
        let mut client = Client::connect(client_addr, server_addr).unwrap();

        let mut read_buf: [u8; 65536] = [0_u8; 1 << 16];

        let data = generate_random_u8_vector(1160);

        assert!(client.send(&data, SendType::Reliable).is_ok());
        thread::sleep(Duration::from_secs(120));
        /*match server.read(&mut read_buf).unwrap() {
            ServerEvent::Receive(1, d) => assert_eq!(data, d),
            _ => panic!(""),
        }*/
    }

    #[test]
    fn soak_test() {
        env::set_var("RUST_LOG", "INFO");
        env_logger::init();

        const MESSAGE_COUNT: usize = 20;
        const CLIENT_COUNT: usize = 10;
        let read_timeout = Duration::from_secs(20);
        //start up the server
        let server_addr = "127.0.0.1:9090".parse().unwrap();
        let mut server = Server::start(server_addr, 64).unwrap();
        let mut read_buf: [u8; MAX_FRAGMENT_SIZE] = [0_u8; MAX_FRAGMENT_SIZE];

        //connect 10 clients to it
        let mut clients = Vec::with_capacity(CLIENT_COUNT);
        for client_index in 0..CLIENT_COUNT as u32 {
            let client_addr = format!("127.0.0.1:{}", 9091 + client_index)
                .parse()
                .unwrap();

            let client = Client::connect(client_addr, server_addr);
            assert!(client.is_ok());
            let client = client.unwrap();

            let read_result = server.read(&mut read_buf, read_timeout);
            assert!(read_result.is_ok());

            if let Ok(Some(ServerEvent::NewConnection(connection_id))) = read_result {
                assert_eq!(connection_id, client_index + 1);
            } else {
                panic!("expected new connection, got: {:?}", read_result.unwrap());
            }

            clients.push(client);
        }

        for i in 0..=0 {
            let client_index = rand::thread_rng().gen_range(0..clients.len());
            let mut client = &mut clients[client_index];

            //start sending reliable messages
            let mut data_list = Vec::with_capacity(MESSAGE_COUNT);
            for i in 0..MESSAGE_COUNT {
                let length = rand::thread_rng().gen_range(if i % 2 == 0 {
                    10..FRAGMENT_SIZE
                    //500..501
                } else {
                    FRAGMENT_SIZE + 1..MAX_FRAGMENT_SIZE
                    //1200..1201
                });
                let data = generate_random_u8_vector(length);
                assert!(client.send(&data, SendType::Reliable).is_ok());
                data_list.push(data);

                thread::sleep(Duration::from_millis(50));
            }

            //receive the data
            for i in 0..MESSAGE_COUNT {
                let ev = server.read(&mut read_buf, read_timeout);
                if let Ok(Some(ServerEvent::Receive(connection_id, data))) = ev {
                    assert_eq!(connection_id, client_index as u32 + 1);
                    assert!(data_list.iter().any(|f| f == data))
                } else {
                    panic!("unexpected server read reading event ({i}) {:?}", ev);
                }
            }

            thread::sleep(Duration::from_secs(1));
        }

        //disconnect the clients and expect lost connections
        for client in &clients {
            assert!(client.disconnect().is_ok());
        }

        for _ in 0..CLIENT_COUNT {
            let read_result = server.read(&mut read_buf, read_timeout);
            assert!(read_result.is_ok());

            if let Ok(Some(ServerEvent::ConnectionLost(connection_id))) = read_result {
                assert!((1..=10).contains(&connection_id));
            } else {
                panic!("expected lost connection, got: {:?}", read_result.unwrap());
            }
        }
    }

    fn generate_random_u8_vector(length: usize) -> Bytes {
        let mut rng = rand::thread_rng();
        let mut result = Vec::with_capacity(length);

        for _ in 0..length {
            let random_u8: u8 = rng.gen();
            result.push(random_u8);
        }

        result
    }
}
