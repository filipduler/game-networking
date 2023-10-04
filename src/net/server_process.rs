use std::{
    collections::{HashMap, VecDeque},
    error, io,
    net::SocketAddr,
    sync::Arc,
    thread::{self},
    time::{Duration, Instant},
};

use anyhow::bail;
use crossbeam_channel::{select, Receiver, Sender};
use log::error;

use super::{
    array_pool::ArrayPool,
    channel::ReadPayload,
    connections::ConnectionManager,
    header::SendType,
    packets::SendEvent,
    socket::{Socket, UdpEvent, UdpSendEvent},
};

pub enum ServerEvent {
    Start,
    Connect,
    Disconnect,
    Receive(u32, Vec<u8>),
}

pub struct ServerProcess {
    socket: Socket,
    //API channels
    out_events: Sender<ServerEvent>,
    in_sends: Receiver<(SocketAddr, SendEvent, SendType)>,
    //connections
    send_queue: VecDeque<UdpSendEvent>,
    connection_manager: ConnectionManager,
}

impl ServerProcess {
    pub fn bind(
        addr: SocketAddr,
        max_clients: usize,
        out_events: Sender<ServerEvent>,
        in_sends: Receiver<(SocketAddr, SendEvent, SendType)>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            socket: Socket::bind(addr)?,
            connection_manager: ConnectionManager::new(max_clients),
            in_sends,
            send_queue: VecDeque::new(),
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
                            msg.1,
                            msg.2
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
                            UdpEvent::Start => {
                                self.out_events.send(ServerEvent::Start)?;
                            }
                            UdpEvent::Read(addr, buffer) => {
                                if let Err(ref e) = self.process_read_request(addr, buffer.used_data()) {
                                    error!("failed processing read request: {e}");
                                };
                            }
                            UdpEvent::SentServer(addr, seq, sent_at) => {
                                if let Some(conn) = self.connection_manager.get_client_mut(&addr) {
                                    conn.channel.send_buffer.mark_sent(seq, sent_at);
                                }
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
        //client exists, process the request
        let mut disconnect_client_addr = None;

        if let Some(client) = self.connection_manager.get_client_mut(&addr) {
            match client.channel.read(data) {
                Ok(ReadPayload::Ref(payload)) => {
                    self.out_events
                        .send(ServerEvent::Receive(client.identity.id, payload.to_vec()))?;
                }
                Ok(ReadPayload::Vec(payload)) => {
                    self.out_events
                        .send(ServerEvent::Receive(client.identity.id, payload))?;
                }
                Err(e) => {
                    error!("failed channel read: {e}");
                    disconnect_client_addr = Some(client.identity.addr);
                }
                _ => {}
            }
        }
        //client doesn't exist and theres space on the server, start the connection process
        else if let Ok(Some(buffer)) = self.connection_manager.process_connect(&addr, data) {
            self.send_queue
                .push_front(UdpSendEvent::Server(buffer, addr));
        }

        //disconnect the client
        if let Some(addr) = disconnect_client_addr {
            self.connection_manager.disconnect_connection(addr);
        }

        Ok(())
    }

    fn process_send_request(
        &mut self,
        addr: SocketAddr,
        send_event: SendEvent,
        send_type: SendType,
    ) -> anyhow::Result<()> {
        if let Some(connection) = self.connection_manager.get_client_mut(&addr) {
            match send_type {
                SendType::Reliable => connection
                    .channel
                    .send_reliable(send_event, &mut self.send_queue)?,
                SendType::Unreliable => connection
                    .channel
                    .send_unreliable(send_event, &mut self.send_queue)?,
            }
        }

        Ok(())
    }

    fn update(&mut self) {
        self.connection_manager.update(&mut self.send_queue);
    }
}
