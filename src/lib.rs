mod io;

#[cfg(test)]
mod tests {
    use crate::io::{client::Client, header::SendType, server::Server};

    use super::*;

    #[test]
    fn it_works() {
        let client_addr = "127.0.0.1:9091".parse().unwrap();
        let server_addr = "127.0.0.1:9090".parse().unwrap();

        let mut server = Server::start(server_addr, 64).unwrap();
        let mut client = Client::connect(client_addr, server_addr).unwrap();

        let data = vec![1, 3, 4];

        //to establish connection
        client.send(data.clone()).unwrap();
        _ = client.read();

        server.send(client_addr, &data, SendType::Reliable);
        _ = client.read();
        loop {
            _ = client.read();
        }
    }
}
