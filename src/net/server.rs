use std::{io, net::SocketAddr, sync::Arc, thread, time::Duration};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use log::error;

use super::{
    array_pool::ArrayPool,
    fragmentation_manager::FragmentationManager,
    header::SendType,
    server_process::{ServerEvent, ServerProcess},
};

pub struct Server {
    in_sends: Sender<(SocketAddr, Vec<u8>, SendType)>,
    out_events: Receiver<ServerEvent>,
}

impl Server {
    pub fn start(addr: SocketAddr, max_clients: usize) -> anyhow::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(
            move || match ServerProcess::bind(addr, max_clients, send_tx, recv_rx) {
                Ok(mut process) => {
                    if let Err(e) = process.start() {
                        error!("error while running starting: {}", e)
                    }
                }
                Err(e) => error!("error while binding process: {}", e),
            },
        );

        //wait for the start event
        match send_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ServerEvent::Start) => {}
            _ => panic!("failed waiting for start event"),
        };

        Ok(Server {
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, addr: SocketAddr, data: &[u8], send_type: SendType) -> anyhow::Result<()> {
        if FragmentationManager::exceeds_max_length(data.len()) {
            bail!("packets of this size arent supported");
        }
        self.in_sends.send((addr, data.to_vec(), send_type))?;
        Ok(())
    }

    pub fn read(&self) -> anyhow::Result<ServerEvent> {
        Ok(self.out_events.recv()?)
    }
}
