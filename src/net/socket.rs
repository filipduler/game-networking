use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{info, warn};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::net::{bytes, MAGIC_NUMBER_HEADER};

use super::send_buffer::SendPayload;
use super::Bytes;

const UDP_SOCKET: Token = Token(0);

pub enum UdpEvent {
    SentServer(SocketAddr, u16, Instant),
    SentClient(u16, Instant),
    Read(SocketAddr, Bytes, Instant),
}

pub enum UdpSendEvent {
    ServerTracking(Bytes, SocketAddr, u16),
    Server(Bytes, SocketAddr),
    ClientTracking(Bytes, u16),
    Client(Bytes),
}

pub struct Socket {
    addr: SocketAddr,
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
            addr,
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
            while let Some(packet) = send_events.pop_back() {
                self.send_queue.push_front(packet);
            }
        }
    }

    pub fn process(
        &mut self,
        deadline: Instant,
        max_events: Option<usize>,
        events: &mut VecDeque<UdpEvent>,
    ) -> anyhow::Result<()> {
        let max_events = max_events.unwrap_or(usize::MAX);

        while Instant::now() < deadline {
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

                            while let Some(packet) = self.send_queue.pop_back() {
                                let send_result = match packet {
                                    UdpSendEvent::ServerTracking(ref data, addr, _) => {
                                        self.socket.send_to(data, addr)
                                    }
                                    UdpSendEvent::ClientTracking(ref data, _) => {
                                        self.socket.send(data)
                                    }
                                    UdpSendEvent::Server(ref data, addr) => {
                                        self.socket.send_to(data, addr)
                                    }
                                    UdpSendEvent::Client(ref data) => self.socket.send(data),
                                };

                                match send_result {
                                    Ok(length) => {
                                        info!("sent packet of size {length} on {}", self.addr);

                                        match packet {
                                            UdpSendEvent::ServerTracking(_, addr, seq) => {
                                                events.push_front(UdpEvent::SentServer(
                                                    addr,
                                                    seq,
                                                    Instant::now(),
                                                ));
                                            }
                                            UdpSendEvent::ClientTracking(_, seq) => {
                                                events.push_front(UdpEvent::SentClient(
                                                    seq,
                                                    Instant::now(),
                                                ));
                                            }
                                            _ => {}
                                        };
                                    }
                                    Err(ref e) if would_block(e) => {
                                        //set the message back in the queue
                                        self.send_queue.push_back(packet);

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
                                            info!(
                                                "received packet of size {packet_size} on {}",
                                                self.addr
                                            );
                                            let data_size = packet_size - 4;
                                            let mut buffer = bytes!(data_size);

                                            //copy the data
                                            buffer[..data_size]
                                                .copy_from_slice(&self.buf[4..packet_size]);

                                            events.push_front(UdpEvent::Read(
                                                source_address,
                                                buffer,
                                                Instant::now(),
                                            ));

                                            if max_events <= events.len() {
                                                return Ok(());
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

        Ok(())
    }
}

fn would_block(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock
}
