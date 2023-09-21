use std::{io, net::SocketAddr, thread, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use log::error;
use mio::net::UdpSocket;

use super::{
    channel::Channel,
    header::{Header, SendType, HEADER_SIZE},
    inner_server::{Event, MessageType},
    socket::{run_udp_socket, UdpEvent},
    MAGIC_NUMBER_HEADER,
};

pub struct Client {
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    channel: Channel,
    //UDP channels
    send_tx: Sender<(SocketAddr, u32, Vec<u8>)>,
    recv_rx: Receiver<UdpEvent>,
}

impl Client {
    pub fn connect(local_addr: SocketAddr, remote_addr: SocketAddr) -> io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            if let Err(e) = run_udp_socket(local_addr, Some(remote_addr), send_rx, recv_tx) {
                error!("error while running udp server: {}", e)
            }
        });

        match recv_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(UdpEvent::Start) => {}
            _ => panic!("failed waiting for start event"),
        };

        Ok(Client {
            local_addr,
            remote_addr,
            channel: Channel::new(local_addr, &send_tx),
            send_tx,
            recv_rx,
        })
    }

    pub fn send(&mut self, data: Vec<u8>) {
        self.channel.send_reliable(Some(&data));
    }

    pub fn read(&mut self) -> Option<Vec<u8>> {
        match self.recv_rx.recv() {
            Ok(event) => match event {
                UdpEvent::Read(_, data) => {
                    self.channel.read(&data);
                }
                UdpEvent::Sent(_, seq, sent_at) => {
                    self.channel.send_buffer.mark_sent(seq, sent_at);
                }
                _ => {}
            },
            Err(e) => panic!("error receiving {e}"),
        }
        /*let packet_size = self.socket.recv(&mut self.buf).unwrap();
        println!("received packet or length {packet_size}");

        if packet_size >= 4 + HEADER_SIZE && self.buf[..4] == MAGIC_NUMBER_HEADER {
            let data_length = packet_size - HEADER_SIZE - 4;

            self.channel.read(&self.buf[4..packet_size]);

            if data_length > 0 {
                let mut vec = vec![0; packet_size];
                let src = &self.buf[4 + HEADER_SIZE..packet_size];
                vec[..data_length].copy_from_slice(src);
                return Some(vec);
            }
        }*/
        None
    }
}
