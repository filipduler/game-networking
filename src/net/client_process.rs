use std::{
    collections::{HashMap, VecDeque},
    io,
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
    thread::{self},
    time::{Duration, Instant},
};

use anyhow::bail;
use crossbeam_channel::{select, Receiver, Sender};
use log::{error, info, warn};
use mio::{net::UdpSocket, Token};
use rand::Rng;

use super::{
    channel::{Channel, ChannelType, ReadPayload},
    connections::{self, ConnectionHandshake},
    header::SendType,
    int_buffer::IntBuffer,
    packets::SendEvent,
    send_buffer::SendPayload,
    socket::{Socket, UdpEvent, UdpSendEvent},
    Bytes, PacketType, MAGIC_NUMBER_HEADER,
};

#[derive(PartialEq, Eq)]
enum ClientState {
    Connected,
    Disconnecting,
}

pub enum InternalClientEvent {
    Connect(u32),
    Receive(Bytes),
    ReceiveParts(Vec<Bytes>),
}

pub struct ClientProcess {
    state: ClientState,
    channel: Channel,
    socket: Socket,
    send_queue: VecDeque<UdpSendEvent>,
    //API channels
    out_events: Sender<InternalClientEvent>,
    in_sends: Receiver<SendEvent>,
    marked_packets_buf: Vec<Rc<SendPayload>>,
}

impl ClientProcess {
    pub fn connect(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        out_events: Sender<InternalClientEvent>,
        in_sends: Receiver<SendEvent>,
    ) -> anyhow::Result<Self> {
        let mut socket = Socket::connect(local_addr, remote_addr)?;

        let connection_response = ConnectionHandshake::new(&mut socket).try_login()?;

        out_events.send(InternalClientEvent::Connect(
            connection_response.connection_id,
        ))?;

        Ok(Self {
            state: ClientState::Connected,
            channel: Channel::new(
                local_addr,
                connection_response.session_key,
                ChannelType::Client,
            ),
            socket,
            send_queue: VecDeque::new(),
            in_sends,
            out_events,
            marked_packets_buf: Vec::new(),
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
                //send requests coming from the API
                recv(self.in_sends) -> msg_result => {
                    //prioritize update
                    if interval_rx.try_recv().is_ok() {
                        self.update();
                    }
                    match msg_result {
                        Ok(msg) => {
                            if let Err(e) = self.process_send_request(msg) {
                                warn!("failed processing send request: {e}")
                            }
                    },
                        Err(e) => bail!("process ending {}", e),
                    };
                }
                //incoming read packets
                default => {
                    if !self.send_queue.is_empty() {
                        self.socket.enqueue_send_events(&mut self.send_queue);
                    }

                    self.socket.process(
                        Instant::now() + Duration::from_millis(10),
                        None,
                        &mut udp_events,
                    )?;

                    //we just processed the disconnect packets and we can finish the loop
                    if self.state == ClientState::Disconnecting {
                        return Ok(());
                    }

                    while let Some(udp_event) = udp_events.pop_back() {
                        match udp_event {
                            UdpEvent::Read(addr, buffer, received_at) => {
                                if let Err(ref e) = self.process_read_request(addr, buffer, &received_at) {
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

    fn process_read_request(
        &mut self,
        addr: SocketAddr,
        buffer: Bytes,
        received_at: &Instant,
    ) -> anyhow::Result<()> {
        match self.channel.read(buffer, received_at)? {
            ReadPayload::Single(payload) => self
                .out_events
                .send(InternalClientEvent::Receive(payload))?,
            ReadPayload::Parts(parts) => self
                .out_events
                .send(InternalClientEvent::ReceiveParts(parts))?,
            _ => {}
        }

        Ok(())
    }

    fn process_send_request(&mut self, send_event: SendEvent) -> anyhow::Result<()> {
        //clear all other outbound packets if the client is disconnecting
        if let SendEvent::Disconnect = send_event {
            self.socket.empty_send_events();
            self.state = ClientState::Disconnecting;
        }

        self.channel.send_event(send_event, &mut self.send_queue)
    }

    fn update(&mut self) {
        if self.state != ClientState::Connected {
            return;
        }

        if let Err(e) = self
            .channel
            .update(&mut self.marked_packets_buf, &mut self.send_queue)
        {
            error!("error updating channel: {e}");
        }
    }
}
