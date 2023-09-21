use std::{io, net::SocketAddr, thread, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use log::error;
use mio::net::UdpSocket;

use super::{
    channel::Channel,
    client_process::{ClientEvent, ClientProcess},
    header::{Header, SendType, HEADER_SIZE},
    socket::{run_udp_socket, UdpEvent},
    MAGIC_NUMBER_HEADER,
};

pub struct Client {
    in_sends: Sender<(Vec<u8>, SendType)>,
    out_events: Receiver<ClientEvent>,
}

impl Client {
    pub fn connect(local_addr: SocketAddr, remote_addr: SocketAddr) -> io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            match ClientProcess::connect(local_addr, remote_addr, send_tx, recv_rx) {
                Ok(mut process) => {
                    if let Err(e) = process.start() {
                        error!("error while running starting: {}", e)
                    }
                }
                Err(e) => error!("error while binding process: {}", e),
            }
        });

        //wait for the start event
        match send_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ClientEvent::Start) => {}
            _ => panic!("failed waiting for start event"),
        };

        Ok(Client {
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, data: &[u8], send_type: SendType) -> anyhow::Result<()> {
        self.in_sends.send((data.to_vec(), send_type))?;
        Ok(())
    }

    pub fn read(&mut self) -> anyhow::Result<Vec<u8>> {
        loop {
            match self.out_events.recv() {
                Ok(ClientEvent::Receive(data)) => return Ok(data),
                Err(e) => panic!("error receiving {e}"),
                _ => panic!("unexpected event"),
            }
        }
    }
}
