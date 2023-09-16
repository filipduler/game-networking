mod io;

#[cfg(test)]
mod tests {
    use crate::io::{client::Client, server::Server};

    use super::*;

    #[test]
    fn it_works() {
        let mut server = Server::start("127.0.0.1:9090".parse().unwrap(), 64).unwrap();
        let mut client = Client::connect(
            "127.0.0.1:9091".parse().unwrap(),
            "127.0.0.1:9090".parse().unwrap(),
        )
        .unwrap();

        let data = vec![1, 3, 4];
        client.send(data).unwrap();
        assert_eq!(4, 4);
    }
}
