use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    thread::{self},
    time::Duration,
};

use crossbeam_channel::{select, Receiver, Sender};
use log::error;
use mio::{net::UdpSocket, Token};

use super::{
    channel::Channel,
    header::SendType,
    socket::{run_udp_socket, ClientSendPacket, UdpEvent, UdpSender},
};

pub enum ClientEvent {
    Start,
    Receive(Vec<u8>),
}

pub struct ClientProcess {
    channel: Channel<ClientSendPacket>,
    //API channels
    out_events: Sender<ClientEvent>,
    in_sends: Receiver<(Vec<u8>, SendType)>,
    //UDP channels
    send_tx: Sender<ClientSendPacket>,
    recv_rx: Receiver<UdpEvent>,
}

impl ClientProcess {
    pub fn connect(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        out_events: Sender<ClientEvent>,
        in_sends: Receiver<(Vec<u8>, SendType)>,
    ) -> std::io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded::<ClientSendPacket>();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            if let Err(e) = run_udp_socket(local_addr, Some(remote_addr), send_rx, recv_tx) {
                error!("error while running udp server: {}", e)
            }
        });

        match recv_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(UdpEvent::Start) => {
                out_events.send(ClientEvent::Start).unwrap();
            }
            _ => panic!("failed waiting for start event"),
        };

        Ok(Self {
            channel: Channel::new(local_addr, &send_tx),
            in_sends,
            out_events,
            send_tx,
            recv_rx,
        })
    }

    pub fn start(&mut self) -> io::Result<()> {
        //to clean up stuff
        let interval_rx = crossbeam_channel::tick(Duration::from_millis(10));

        loop {
            select! {
                recv(interval_rx) -> _ => {
                    self.update();
                }
                //incoming read packets
                recv(self.recv_rx) -> msg_result => {
                    match msg_result {
                        Ok(UdpEvent::Read(addr, data)) => {
                            self.process_read_request(
                                addr,
                                &data,
                            );
                        },
                        Ok(UdpEvent::SentClient(seq, sent_at)) => {
                            self.channel.send_buffer.mark_sent(seq, sent_at);
                        },
                        Err(e) => panic!("panic reading udp event {}", e),
                        _ => {},
                    }

                },
                //send requests coming fron the API
                recv(self.in_sends) -> msg_result => {
                    let msg = msg_result.unwrap();
                    self.process_send_request(
                        msg.0,
                        msg.1
                    );
                }
            }
        }
    }

    fn process_read_request(&mut self, addr: SocketAddr, data: &[u8]) -> anyhow::Result<()> {
        self.channel.read(data);

        self.out_events.send(ClientEvent::Receive(data.to_vec()))?;

        Ok(())
    }

    fn process_send_request(&mut self, data: Vec<u8>, send_type: SendType) {
        self.channel.send_reliable(Some(&data));
    }

    fn update(&mut self) {
        //TODO: add
    }
}
