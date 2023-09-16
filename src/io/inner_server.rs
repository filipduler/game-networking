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
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use log::warn;
use mio::{net::UdpSocket, Events, Interest, Poll, Token};

use super::{
    connection::Connection,
    header::{Header, HEADER_SIZE},
    send_buffer::SendBuffer,
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
    socket: UdpSocket,
    out_events: Sender<Event>,
    in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    connections: HashMap<SocketAddr, Connection>,
    send_queue: VecDeque<(SocketAddr, Rc<SendBuffer>)>,
}

impl Process {
    pub fn bind(
        addr: SocketAddr,
        max_clients: usize,
        out_events: Sender<Event>,
        in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        Ok(Self {
            addr,
            socket,
            in_sends,
            out_events,
            connections: HashMap::with_capacity(max_clients),
            send_queue: VecDeque::new(),
        })
    }

    pub fn start(&mut self) -> io::Result<()> {
        let mut poll = Poll::new()?;
        // Create storage for events. Since we will only register a single socket, a
        // capacity of 1 will do.
        let mut events = Events::with_capacity(1);

        poll.registry()
            .register(&mut self.socket, UDP_SOCKET, Interest::READABLE)?;

        // Initialize a buffer for the UDP packet. We use the maximum size of a UDP
        // packet, which is the maximum value of 16 a bit integer.
        let mut buf = [0; 1 << 16];

        // Our event loop.
        loop {
            //check if there are and send requests in the queue
            if !self.send_queue.is_empty() {
                poll.registry().reregister(
                    &mut self.socket,
                    UDP_SOCKET,
                    Interest::READABLE | Interest::WRITABLE,
                )?;
            }

            // Poll to check if we have events waiting for us.
            if let Err(err) = poll.poll(&mut events, None) {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }

            // Process each event.
            for event in events.iter() {
                match event.token() {
                    UDP_SOCKET => loop {
                        if event.is_writable() && !self.send_queue.is_empty() {
                            while let Some((addr, send_buffer)) = self.send_queue.front() {
                                match self.socket.send_to(&send_buffer.data, *addr) {
                                    Ok(_) => self.send_queue.pop_front(),
                                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                        break;
                                    }
                                    Err(e) => {
                                        return Err(e);
                                    }
                                };
                            }

                            poll.registry().reregister(
                                &mut self.socket,
                                UDP_SOCKET,
                                Interest::READABLE,
                            )?;
                        }

                        if event.is_readable() {
                            // In this loop we receive all packets queued for the socket.
                            match self.socket.recv_from(&mut buf) {
                                Ok((packet_size, source_address)) => {
                                    if packet_size >= 4 && buf[..4] == MAGIC_NUMBER_HEADER {
                                        self.process_read_request(
                                            source_address,
                                            &buf[4..packet_size],
                                        );
                                    }
                                }
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    break;
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                        }
                    },
                    _ => {
                        warn!("Got event for unexpected token: {:?}", event);
                    }
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

        //mark the packet as recieved

        //mark messages as sent
        connection
            .reliable_channel
            .mark_received(header.seq, header.ack, header.ack_bits);

        //TODO: make this sane
        //send ack
        let send_buffer = connection.reliable_channel.send(None);
        self.send_queue.push_back((addr, send_buffer));

        self.out_events
            .send(Event::Receive(connection.identity.id, data.to_vec()))
            .unwrap();
    }

    fn process_send_request(&mut self, addr: SocketAddr, data: Vec<u8>, send_type: SendType) {
        if let Some(connection) = self.connections.get_mut(&addr) {
            let send_buffer = connection.reliable_channel.send(Some(&data));
            self.send_queue.push_back((addr, send_buffer));
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
