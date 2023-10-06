use std::{io, net::SocketAddr, sync::Arc, thread, time::Duration};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use log::error;

use super::{
    array_pool::ArrayPool,
    fragmentation_manager::FragmentationManager,
    header::SendType,
    packets::{self, SendEvent},
    server_process::{InternalServerEvent, ServerProcess},
};

pub enum ServerEvent<'a> {
    Connect,
    Receive(u32, &'a [u8]),
}

pub struct Server {
    in_sends: Sender<(SocketAddr, SendEvent, SendType)>,
    out_events: Receiver<InternalServerEvent>,
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
        match send_rx.recv_timeout(Duration::from_secs(50)) {
            Ok(InternalServerEvent::ServerStarted) => {}
            _ => panic!("failed waiting for start event"),
        };

        Ok(Server {
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, addr: SocketAddr, data: &[u8], send_type: SendType) -> anyhow::Result<()> {
        let send_event = packets::construct_send_event(data)?;

        self.in_sends.send((addr, send_event, send_type))?;
        Ok(())
    }

    pub fn read<'a>(&self, dest: &'a mut [u8]) -> anyhow::Result<ServerEvent<'a>> {
        todo!("this still has to return the event..");
        match self.out_events.recv() {
            Ok(InternalServerEvent::Receive(client_id, buffer)) => {
                if dest.len() < buffer.len() {
                    bail!("destination size is not big enough.")
                }
                dest[..buffer.len()].copy_from_slice(&buffer);
                Ok(ServerEvent::Receive(client_id, &dest[..buffer.len()]))
            }
            Ok(InternalServerEvent::ReceiveParts(client_id, parts)) => {
                let mut bytes_offset = 0;
                for part in parts {
                    let part_len = part.len();

                    if bytes_offset + part_len <= dest.len() {
                        dest[bytes_offset..bytes_offset + part_len].copy_from_slice(&part);
                        bytes_offset += part_len;
                    } else {
                        bail!("destination size is not big enough.")
                    }
                }

                Ok(ServerEvent::Receive(client_id, &dest[..bytes_offset]))
            }
            Err(e) => panic!("error receiving {e}"),
            _ => panic!("unexpected event"),
        }
    }
}
