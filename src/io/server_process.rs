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
    connection::Connection,
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
    addr: SocketAddr,
    connections: HashMap<SocketAddr, Connection>,
    //API channels
    out_events: Sender<ServerEvent>,
    in_sends: Receiver<(SocketAddr, Vec<u8>, SendType)>,
    //UDP channels
    send_tx: Sender<ServerSendPacket>,
    recv_rx: Receiver<UdpEvent>,
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
            addr,
            in_sends,
            out_events,
            connections: HashMap::with_capacity(max_clients),
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
                            if let Some(conn) = self.connections.get_mut(&addr) {
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
        let connection = self.connections.entry(addr).or_insert(Connection::new(
            addr,
            &self.send_tx,
            0, /*TODO: fix */
        ));

        connection.channel.read(data);

        self.out_events
            .send(ServerEvent::Receive(connection.identity.id, data.to_vec()))
            .unwrap();
    }

    fn process_send_request(&mut self, addr: SocketAddr, data: Vec<u8>, send_type: SendType) {
        if let Some(connection) = self.connections.get_mut(&addr) {
            connection.channel.send_reliable(Some(&data));
        }
    }

    fn update(&mut self) {
        for connection in self.connections.values_mut() {
            connection.update()
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
