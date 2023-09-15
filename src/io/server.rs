use std::{io, net::SocketAddr, thread};

use super::inner_server::{Event, Process, SendType};

pub struct Server {}

impl Server {
    pub fn start(host: SocketAddr, max_clients: usize) -> io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::bounded(100);
        let (recv_tx, recv_rx) = crossbeam_channel::bounded(100);

        let mut process = Process::bind(host, max_clients, send_tx, recv_rx)?;

        thread::spawn(move || {
            _ = process.start();
        });

        Ok(Server {})
    }

    pub fn send(&self, addr: SocketAddr, data: &[u8], send_type: SendType) {}

    pub fn recv(&self) -> Option<Event> {
        None
    }
}
