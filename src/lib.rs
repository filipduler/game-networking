mod io;

#[cfg(test)]
mod tests {
    use crate::io::server::Server;

    use super::*;

    #[test]
    fn it_works() {
        let mut server = Server::start("127.0.0.1:9090".parse().unwrap(), 64).unwrap();

        assert_eq!(4, 4);
    }
}
