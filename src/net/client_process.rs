use std::{
    collections::{HashMap, VecDeque},
    io,
    net::SocketAddr,
    sync::Arc,
    thread::{self},
    time::{Duration, Instant},
};

use anyhow::bail;
use crossbeam_channel::{select, Receiver, Sender};
use log::error;
use mio::{net::UdpSocket, Token};
use rand::Rng;

use super::{
    array_pool::ArrayPool,
    channel::{Channel, ChannelType, ReadPayload},
    connections,
    header::SendType,
    int_buffer::IntBuffer,
    packets::SendEvent,
    socket::{Socket, UdpEvent, UdpSendEvent},
    PacketType, MAGIC_NUMBER_HEADER,
};

pub enum ClientEvent {
    Connect(u32),
    Receive(Vec<u8>),
}

pub struct ClientProcess {
    channel: Channel,
    socket: Socket,
    send_queue: VecDeque<UdpSendEvent>,
    //API channels
    out_events: Sender<ClientEvent>,
    in_sends: Receiver<(SendEvent, SendType)>,
}

impl ClientProcess {
    pub fn connect(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        out_events: Sender<ClientEvent>,
        in_sends: Receiver<(SendEvent, SendType)>,
    ) -> anyhow::Result<Self> {
        let mut socket = Socket::connect(local_addr, remote_addr)?;

        let (session_key, client_id) = connections::try_login(&mut socket).expect("login failed");

        out_events.send(ClientEvent::Connect(client_id))?;

        Ok(Self {
            channel: Channel::new(local_addr, session_key, ChannelType::Client),
            socket,
            send_queue: VecDeque::new(),
            in_sends,
            out_events,
        })
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        let interval_rx = crossbeam_channel::tick(Duration::from_millis(10));
        let mut udp_events = VecDeque::new();

        loop {
            select! {
                //constant updates
                recv(interval_rx) -> _ => {
                    self.update();
                }
                //send requests coming fron the API
                recv(self.in_sends) -> msg_result => {
                    //prioritize update
                    if interval_rx.try_recv().is_ok() {
                        self.update();
                    }

                    match msg_result {
                        Ok(msg) => self.process_send_request(
                            msg.0,
                            msg.1
                        ),
                        Err(e) => panic!("panic reading udp event {}", e),
                    };
                }
                //incoming read packets
                default => {
                    self.socket.process(
                        Instant::now() + Duration::from_millis(10),
                        None,
                        &mut udp_events,
                    )?;

                    while let Some(udp_event) = udp_events.pop_back() {
                        match udp_event {
                            UdpEvent::Read(addr, buffer) => {
                                if let Err(ref e) = self.process_read_request(addr, buffer.used_data()) {
                                    error!("failed processing read request: {e}");
                                };
                            }
                            UdpEvent::SentClient(seq, sent_at) => {
                                self.channel.send_buffer.mark_sent(seq, sent_at);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn process_read_request(&mut self, addr: SocketAddr, data: &[u8]) -> anyhow::Result<()> {
        match self.channel.read(data)? {
            ReadPayload::Ref(payload) => self
                .out_events
                .send(ClientEvent::Receive(payload.to_vec()))?,
            ReadPayload::Vec(payload) => self.out_events.send(ClientEvent::Receive(payload))?,
            _ => {}
        }

        Ok(())
    }

    fn process_send_request(
        &mut self,
        send_event: SendEvent,
        send_type: SendType,
    ) -> anyhow::Result<()> {
        match send_type {
            SendType::Reliable => self.channel.send_reliable(send_event)?,
            SendType::Unreliable => self.channel.send_unreliable(send_event)?,
        }

        Ok(())
    }

    fn update(&mut self) {
        if self.channel.send_ack {
            self.channel.send_empty_ack(&mut self.send_queue);
        }
    }
}