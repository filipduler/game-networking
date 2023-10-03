use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::Arc,
    thread::{self},
    time::Duration,
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
    socket::{run_udp_socket, UdpEvent, UdpSendEvent},
    PacketType, MAGIC_NUMBER_HEADER,
};

pub enum ClientEvent {
    Connect(u32),
    Receive(Vec<u8>),
}

pub struct ClientProcess {
    channel: Channel,
    //API channels
    out_events: Sender<ClientEvent>,
    in_sends: Receiver<(SendEvent, SendType)>,
    //UDP channels
    send_tx: Sender<UdpSendEvent>,
    recv_rx: Receiver<UdpEvent>,
    array_pool: Arc<ArrayPool>,
}

impl ClientProcess {
    pub fn connect(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        out_events: Sender<ClientEvent>,
        in_sends: Receiver<(SendEvent, SendType)>,
        array_pool: Arc<ArrayPool>,
    ) -> anyhow::Result<Self> {
        let (send_tx, send_rx) = crossbeam_channel::unbounded();
        let (recv_tx, recv_rx) = crossbeam_channel::unbounded();

        let c_array_pool = array_pool.clone();
        thread::spawn(move || {
            if let Err(e) = run_udp_socket(
                local_addr,
                Some(remote_addr),
                send_rx,
                recv_tx,
                c_array_pool,
            ) {
                error!("error while running udp server: {}", e)
            }
        });

        let (session_key, client_id) =
            connections::try_login(&recv_rx, &send_tx).expect("login failed");

        out_events.send(ClientEvent::Connect(client_id))?;

        Ok(Self {
            channel: Channel::new(
                local_addr,
                session_key,
                ChannelType::Client,
                &send_tx,
                &array_pool,
            ),
            in_sends,
            out_events,
            send_tx,
            recv_rx,
            array_pool,
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
                        Ok(UdpEvent::Read(addr, length, data)) => {
                            if let Err(ref e) = self.process_read_request(
                                addr,
                                &data[..length],
                            ) {
                                error!("failed processing read request: {e}");
                            };
                            self.array_pool.free(data);
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
                    match msg_result {
                        Ok(msg) => self.process_send_request(
                            msg.0,
                            msg.1
                        ),
                        Err(e) => panic!("panic reading udp event {}", e),
                    };
                }
            }
        }
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
            self.channel.send_empty_ack();
        }
    }
}
