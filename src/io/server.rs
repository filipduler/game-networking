use std::{io, net::SocketAddr, thread};

use crossbeam_channel::{Receiver, Sender};
use log::{error, warn};

use super::{
    header::SendType,
    inner_server::{Event, Process},
};

pub struct Server {
    in_sends: Sender<(SocketAddr, Vec<u8>, SendType)>,
    out_events: Receiver<Event>,
}

impl Server {
    pub fn start(host: SocketAddr, max_clients: usize) -> io::Result<Self> {
        env_logger::init();

        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(
            move || match Process::bind(host, max_clients, send_tx, recv_rx) {
                Ok(mut process) => {
                    if let Err(e) = process.start() {
                        error!("error while running starting: {}", e)
                    }
                }
                Err(e) => error!("error while binding process: {}", e),
            },
        );

        Ok(Server {
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, addr: SocketAddr, data: &[u8], send_type: SendType) {
        self.in_sends
            .send((addr, data.to_vec(), send_type))
            .unwrap();
    }

    pub fn recv(&self) -> Option<Event> {
        unimplemented!()
    }
}
