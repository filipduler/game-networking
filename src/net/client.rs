use std::{io, net::SocketAddr, sync::Arc, thread, time::Duration};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use log::error;

use super::{
    array_pool::ArrayPool,
    client_process::{ClientEvent, ClientProcess},
    fragmentation_manager::FragmentationManager,
    header::SendType,
    packets::{self, SendEvent},
};

pub struct Client {
    in_sends: Sender<(SendEvent, SendType)>,
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
        match send_rx.recv_timeout(Duration::from_secs(50)) {
            Ok(ClientEvent::Connect(client_id)) => {}
            _ => panic!("failed waiting for start event"),
        };

        Ok(Client {
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, data: &[u8], send_type: SendType) -> anyhow::Result<()> {
        let send_event = packets::construct_send_event(data)?;

        self.in_sends.send((send_event, send_type))?;
        Ok(())
    }

    pub fn read(&self) -> anyhow::Result<Vec<u8>> {
        match self.out_events.recv() {
            Ok(ClientEvent::Receive(data)) => Ok(data),
            Err(e) => panic!("error receiving {e}"),
            _ => panic!("unexpected event"),
        }
    }
}
