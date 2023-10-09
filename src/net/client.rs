use std::{io, net::SocketAddr, sync::Arc, thread, time::Duration};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use log::error;

use super::{
    client_process::{ClientProcess, InternalClientEvent},
    fragmentation_manager::FragmentationManager,
    header::SendType,
    packets::{self, SendEvent},
};

pub struct Client {
    client_id: u32,
    in_sends: Sender<(SendEvent, SendType)>,
    out_events: Receiver<InternalClientEvent>,
}

impl Client {
    pub fn connect(addr: SocketAddr, remote_addr: SocketAddr) -> io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(
            move || match ClientProcess::connect(addr, remote_addr, send_tx, recv_rx) {
                Ok(mut process) => {
                    if let Err(e) = process.start() {
                        error!("error while running starting: {}", e)
                    }
                }
                Err(e) => error!("error while binding process: {}", e),
            },
        );

        //wait for the start event
        let client_id = match send_rx.recv_timeout(Duration::from_secs(50)) {
            Ok(InternalClientEvent::Connect(client_id)) => client_id,
            _ => panic!("failed waiting for start event"),
        };

        Ok(Client {
            client_id,
            in_sends: recv_tx,
            out_events: send_rx,
        })
    }

    pub fn send(&self, data: &[u8], send_type: SendType) -> anyhow::Result<()> {
        let send_event = packets::construct_send_event(data)?;

        self.in_sends.send((send_event, send_type))?;
        Ok(())
    }

    pub fn read<'a>(&self, dest: &'a mut [u8]) -> anyhow::Result<&'a [u8]> {
        match self.out_events.recv() {
            Ok(InternalClientEvent::Receive(buffer)) => {
                if dest.len() < buffer.len() {
                    bail!("destination size is not big enough.")
                }
                dest[..buffer.len()].copy_from_slice(&buffer);
                Ok(&dest[..buffer.len()])
            }
            Ok(InternalClientEvent::ReceiveParts(parts)) => {
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

                Ok(&dest[..bytes_offset])
            }
            Err(e) => panic!("error receiving {e}"),
            _ => panic!("unexpected event"),
        }
    }
}
