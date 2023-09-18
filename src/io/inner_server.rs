use std::{
    collections::{
        hash_map::{OccupiedEntry, VacantEntry},
        HashMap, VecDeque,
    },
    io,
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::io::MAGIC_NUMBER_HEADER;
use crossbeam_channel::{select, Receiver, Sender, TryRecvError, TrySendError};
use log::{error, warn};
use mio::{net::UdpSocket, Events, Interest, Poll, Token};

use super::{
    connection::Connection,
    header::{Header, HEADER_SIZE},
    send_buffer::SendBuffer,
    socket::{run_udp_socket, UdpEvent},
};

// A token to allow us to identify which event is for the `UdpSocket`.
const UDP_SOCKET: Token = Token(0);
pub enum Event {
    Connect,
    Disconnect,
    Receive(u32, Vec<u8>),
}

#[repr(u8)]
pub enum MessageType {
    ConnectionRequest = 1,
    ConnectionDenied = 2,
    Challange = 3,
    ChallangeResponse = 4,
    ConnectionPayload = 5,
    Disconnect = 6,
}

#[derive(Clone, Copy)]
pub enum SendType {
    Reliable,
    Unreliable,
}

pub struct Process {
    addr: SocketAddr,
    connections: HashMap<SocketAddr, Connection>,
    //API channels
    out_events: Sender<Event>,
    in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    //UDP channels
    send_rx: Receiver<(SocketAddr, u32, Vec<u8>)>,
    send_tx: Sender<(SocketAddr, u32, Vec<u8>)>,
    recv_rx: Receiver<UdpEvent>,
    recv_tx: Sender<UdpEvent>,
}

impl Process {
    pub fn bind(
        addr: SocketAddr,
        max_clients: usize,
        out_events: Sender<Event>,
        in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    ) -> std::io::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        Ok(Self {
            addr,
            in_sends,
            out_events,
            connections: HashMap::with_capacity(max_clients),
            send_tx,
            send_rx,
            recv_tx,
            recv_rx,
        })
    }

    pub fn start(&mut self) -> io::Result<()> {
        let s_send_rx = self.send_rx.clone();
        let s_recv_tx = self.recv_tx.clone();
        let s_addr = self.addr;
        thread::spawn(move || {
            if let Err(e) = run_udp_socket(s_addr, s_send_rx, s_recv_tx) {
                error!("error while running udp server: {}", e)
            }
        });

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
                        Ok(UdpEvent::Sent(addr, seq, sent_at)) => {
                            if let Some(conn) = self.connections.get_mut(&addr) {
                                conn.reliable_channel.send_buffer.mark_sent(seq, sent_at);
                            }
                        },
                        Err(e) => panic!("panic reading udp event {}", e)
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
        let connection = self
            .connections
            .entry(addr)
            .or_insert(Connection::new(addr, 0 /*TODO: fix */));

        let header = Header::read(data);
        //TODO: check if its duplicate

        connection.reliable_channel.update_remote_seq(header.seq);
        //mark messages as sent
        connection
            .reliable_channel
            .mark_sent_packets(header.ack, header.ack_bits);

        //send ack
        connection.reliable_channel.send_ack = true;

        self.out_events
            .send(Event::Receive(connection.identity.id, data.to_vec()))
            .unwrap();
    }

    fn process_send_request(&mut self, addr: SocketAddr, data: Vec<u8>, send_type: SendType) {
        if let Some(connection) = self.connections.get_mut(&addr) {
            let send_buffer = connection.reliable_channel.create_send_buffer(Some(&data));
            self.send_tx
                .send((addr, send_buffer.seq, send_buffer.data.to_vec()))
                .unwrap();
        }
    }

    fn update(&mut self) {
        for connection in self.connections.values_mut() {
            if connection.reliable_channel.send_ack {
                let send_buffer = connection.reliable_channel.create_send_buffer(None);
                self.send_tx
                    .send((
                        connection.identity.addr,
                        send_buffer.seq,
                        send_buffer.data.to_vec(),
                    ))
                    .unwrap();
                connection.reliable_channel.send_ack = false;
            }
        }
    }

    /*pub fn start(mut self, max_clients: usize, addr: SocketAddr) -> std::io::Result<Self> {
         let mut poll = Poll::new()?;
         let mut events = Events::with_capacity(1);

         let (send_tx, send_rx) = crossbeam_channel::bounded(100);
         let (recv_tx, recv_rx) = crossbeam_channel::bounded(100);
         let join = thread::spawn(|| {
             if let Err(err) = run(recv_rx, send_tx) {
                 println!("err {}", err);
             }
         });

         Ok(Server {
             addr,
             udp_thread_join: join,
             sender: recv_tx,
             receiver: send_rx,
             connections: HashMap::new(),
         })
     }

    pub fn process(&mut self, start: &Instant, timeout: &Duration) -> Option<Event> {
         let deadline = *start + *timeout;
         loop {
             if start.elapsed() >= *timeout {
                 break;
             }

             if let Ok(packet) = self.receiver.recv_deadline(deadline) {
                 if packet.1.is_empty() {
                     continue;
                 }

                 //TODO: process packet
                 if let Some(message_type) = extract_message_type(packet.1[0]) {
                     match message_type {
                         MessageType::ChallangeResponse => {
                             self.process_challange_response(&packet.1, packet.0)
                         }
                         MessageType::ConnectionRequest => {
                             self.process_connection_request(&packet.1, packet.0)
                         }

                         _ => {}
                     };
                 }
             }
         }
         None
     }

     fn process_challange_response(&mut self, data: &[u8], addr: SocketAddr) {
         if let Some(connection) = self.connections.get_mut(&addr) {
             let mut buffer = IntBuffer { index: 1 };
             if buffer.read_u64(data) == connection.session_key {
                 connection.accepted = true;
             }
         }
     }

     fn process_connection_request(&mut self, data: &[u8], addr: SocketAddr) {
         let mut buffer = IntBuffer { index: 0 };
         let client_salt = buffer.read_u64(data);

         let connection = self
             .connections
             .entry(addr)
             .and_modify(|conn| {
                 conn.client_salt = client_salt;
                 conn.session_key = client_salt ^ conn.server_salt;
             })
             .or_insert(Connection::new(addr, client_salt));

         buffer.index = 0;
         let mut data = [0_u8; 21];

         buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut data);
         buffer.write_u8(MessageType::Challange as u8, &mut data);
         buffer.write_u64(connection.client_salt, &mut data);
         buffer.write_u64(connection.server_salt, &mut data);

         //TODO: handle errors
         self.sender.send((addr, data.to_vec())).unwrap()
     }*/
}

fn extract_message_type(value: u8) -> Option<MessageType> {
    match value {
        1 => Some(MessageType::ConnectionRequest),
        2 => Some(MessageType::ConnectionDenied),
        3 => Some(MessageType::Challange),
        4 => Some(MessageType::ChallangeResponse),
        5 => Some(MessageType::ConnectionPayload),
        6 => Some(MessageType::Disconnect),
        _ => None,
    }
}
