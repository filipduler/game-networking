use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{info, warn};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::net::MAGIC_NUMBER_HEADER;

use super::array_pool::{ArrayPool, BufferPoolRef};

const UDP_SOCKET: Token = Token(0);

pub enum UdpEvent {
    Start,
    SentServer(SocketAddr, u16, Instant),
    SentClient(u16, Instant),
    Read(SocketAddr, BufferPoolRef),
}

pub enum UdpSendEvent {
    Server(BufferPoolRef, SocketAddr, u16, bool),
    Client(BufferPoolRef, u16, bool),
}

pub struct Socket {
    poll: Poll,
    events: Events,
    socket: UdpSocket,
    client_mode: bool,
    send_queue: VecDeque<UdpSendEvent>,
    buf: [u8; 1 << 16],
}

impl Socket {
    pub fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
        let mut poll = Poll::new()?;
        let mut socket = UdpSocket::bind(addr)?;
        poll.registry()
            .register(&mut socket, UDP_SOCKET, Interest::READABLE)?;

        Ok(Self {
            poll,
            socket,
            events: Events::with_capacity(1),
            client_mode: false,
            send_queue: VecDeque::new(),
            buf: [0; 1 << 16],
        })
    }

    pub fn connect(addr: SocketAddr, remote_addr: SocketAddr) -> anyhow::Result<Self> {
        let mut socket = Socket::bind(addr)?;
        socket.socket.connect(remote_addr)?;
        socket.client_mode = true;

        Ok(socket)
    }

    pub fn enqueue_send_event(&mut self, send_event: UdpSendEvent) {
        self.send_queue.push_front(send_event);
    }

    pub fn enqueue_send_events(&mut self, send_events: &mut VecDeque<UdpSendEvent>) {
        if self.send_queue.is_empty() {
            std::mem::swap(send_events, &mut self.send_queue);
        } else {
            //TODO: test this
            while let Some(packet) = send_events.pop_back() {
                self.send_queue.push_back(packet);
            }
        }
    }

    pub fn process(
        &mut self,
        deadline: Instant,
        max_events: Option<usize>,
        events: &mut VecDeque<UdpEvent>,
    ) -> anyhow::Result<()> {
        loop {
            let timeout = deadline - Instant::now();

            //check if there are and send requests
            if !self.send_queue.is_empty() {
                self.poll.registry().reregister(
                    &mut self.socket,
                    UDP_SOCKET,
                    Interest::READABLE | Interest::WRITABLE,
                )?;
            }

            // Poll to check if we have events waiting for us.
            if let Err(err) = self.poll.poll(&mut self.events, Some(timeout)) {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err.into());
            }

            // Process each event.
            for event in self.events.iter() {
                match event.token() {
                    UDP_SOCKET => {
                        if event.is_writable() {
                            let mut send_finished = true;

                            while let Some(packet) = self.send_queue.front() {
                                let send_result = match packet {
                                    UdpSendEvent::Server(ref data, addr, _, _) => {
                                        self.socket.send_to(data.used_data(), *addr)
                                    }
                                    UdpSendEvent::Client(ref data, _, _) => {
                                        self.socket.send(data.used_data())
                                    }
                                };

                                match send_result {
                                    Ok(length) => {
                                        info!("sent packet of size {length}");

                                        match packet {
                                            UdpSendEvent::Server(data, addr, seq, track) => {
                                                if *track {
                                                    events.push_front(UdpEvent::SentServer(
                                                        *addr,
                                                        *seq,
                                                        Instant::now(),
                                                    ));
                                                }
                                            }
                                            UdpSendEvent::Client(data, seq, track) => {
                                                if *track {
                                                    events.push_front(UdpEvent::SentClient(
                                                        *seq,
                                                        Instant::now(),
                                                    ));
                                                }
                                            }
                                        };
                                    }
                                    Err(ref e) if would_block(e) => {
                                        break;
                                    }
                                    Err(e) => {
                                        return Err(e.into());
                                    }
                                    _ => {}
                                };
                            }

                            //if we sent all of the packets in the channel we can switch back to readable events
                            if self.send_queue.is_empty() {
                                self.poll.registry().reregister(
                                    &mut self.socket,
                                    UDP_SOCKET,
                                    Interest::READABLE,
                                )?;
                            }
                        } else if event.is_readable() {
                            // In this loop we receive all packets queued for the socket.
                            loop {
                                match self.socket.recv_from(&mut self.buf) {
                                    Ok((packet_size, source_address)) => {
                                        if packet_size >= 4 && self.buf[..4] == MAGIC_NUMBER_HEADER
                                        {
                                            info!("received packet of size {packet_size}");
                                            let data_size = packet_size - 4;
                                            let mut buffer = ArrayPool::rent(data_size);

                                            //copy the data
                                            buffer.copy_slice(&self.buf[4..packet_size]);

                                            events
                                                .push_front(UdpEvent::Read(source_address, buffer));

                                            if let Some(max_events) = max_events {
                                                if max_events >= events.len() {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    Err(ref e) if would_block(e) => break,
                                    Err(e) => {
                                        return Err(e.into());
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        warn!("Got event for unexpected token: {:?}", event);
                    }
                }
            }
        }
    }
}

fn would_block(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock
}
