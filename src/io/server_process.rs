use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    thread::{self},
    time::Duration,
};

use crossbeam_channel::{select, Receiver, Sender};
use log::error;

use super::{
    connection::ConnectionManager,
    header::SendType,
    socket::{run_udp_socket, ServerSendPacket, UdpEvent},
};

pub enum ServerEvent {
    Start,
    Connect,
    Disconnect,
    Receive(u32, Vec<u8>),
}

pub struct ServerProcess {
    //API channels
    out_events: Sender<ServerEvent>,
    in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    //UDP channels
    send_tx: Sender<ServerSendPacket>,
    recv_rx: Receiver<UdpEvent>,
    //connections
    connection_manager: ConnectionManager,
}

impl ServerProcess {
    pub fn bind(
        addr: SocketAddr,
        max_clients: usize,
        out_events: Sender<ServerEvent>,
        in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    ) -> std::io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            if let Err(e) = run_udp_socket(addr, None, send_rx, recv_tx) {
                error!("error while running udp server: {}", e)
            }
        });

        match recv_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(UdpEvent::Start) => {
                out_events.send(ServerEvent::Start).unwrap();
            }
            _ => panic!("failed waiting for start event"),
        };

        Ok(Self {
            connection_manager: ConnectionManager::new(max_clients, &send_tx),
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
                        Ok(UdpEvent::SentServer(addr, seq, sent_at)) => {
                            if let Some(conn) = self.connection_manager.get_client_mut(&addr) {
                                conn.channel.send_buffer.mark_sent(seq, sent_at);
                            }
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
                        msg.1,
                        msg.2
                    );
                }
            }
        }
    }

    fn process_read_request(&mut self, addr: SocketAddr, data: &[u8]) {
        //client exists, process the request
        if let Some(client) = self.connection_manager.get_client_mut(&addr) {
            client.channel.read(data);

            //TODO: handle unwrap
            self.out_events
                .send(ServerEvent::Receive(client.identity.id, data.to_vec()))
                .unwrap();
        }
        //client doesn't exist and theres space on the server, start the connection process
        else if let Ok(Some(payload)) = self.connection_manager.process_connect(&addr, data) {
            //TODO: handle unwrap
            self.send_tx
                .send(ServerSendPacket::new_non_delivery(payload, addr))
                .unwrap();
        }
    }

    fn process_send_request(&mut self, addr: SocketAddr, data: Vec<u8>, send_type: SendType) {
        if let Some(connection) = self.connection_manager.get_client_mut(&addr) {
            connection.channel.send_reliable(Some(&data));
        }
    }

    fn update(&mut self) {
        self.connection_manager.update();
    }
}
