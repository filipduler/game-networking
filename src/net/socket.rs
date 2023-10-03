use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{info, warn};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use std::borrow::BorrowMut;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::net::MAGIC_NUMBER_HEADER;

use super::array_pool::ArrayPool;

#[derive(PartialEq, Eq)]
pub enum UdpEvent {
    Start,
    SentServer(SocketAddr, u16, Instant),
    SentClient(u16, Instant),
    Read(SocketAddr, usize, Vec<u8>),
}

#[derive(Clone)]
pub enum UdpSendEvent {
    Server(Vec<u8>, usize, SocketAddr, u16),
    ServerNonTracking(Vec<u8>, usize, SocketAddr),
    Client(Vec<u8>, usize, u16),
    ClientNonTracking(Vec<u8>, usize),
}

// A token to allow us to identify which event is for the `UdpSocket`.
const UDP_SOCKET: Token = Token(0);

pub fn run_udp_socket(
    local_addr: SocketAddr,
    remote_addr_opt: Option<SocketAddr>,
    send_receiver: Receiver<UdpSendEvent>,
    event_sender: Sender<UdpEvent>,
    array_pool: Arc<ArrayPool>,
) -> anyhow::Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1);

    let mut socket = UdpSocket::bind(local_addr)?;

    let client_mode = remote_addr_opt.is_some();
    if let Some(remote_addr) = remote_addr_opt {
        socket.connect(remote_addr)?;
    }

    //emit on start event
    event_sender.send(UdpEvent::Start)?;

    poll.registry()
        .register(&mut socket, UDP_SOCKET, Interest::READABLE)?;

    let mut buf = [0; 1 << 16];

    let mut unsent_packet = None::<UdpSendEvent>;
    let timeout = Duration::from_millis(1);

    // Our event loop.
    loop {
        //check if there are and send requests
        if !send_receiver.is_empty() {
            poll.registry().reregister(
                &mut socket,
                UDP_SOCKET,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        }

        // Poll to check if we have events waiting for us.
        if let Err(err) = poll.poll(&mut events, Some(timeout)) {
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err.into());
        }

        // Process each event.
        for event in events.iter() {
            match event.token() {
                UDP_SOCKET => {
                    if event.is_writable() {
                        let mut send_finished = true;

                        loop {
                            let data = if let Some(ref packet) = unsent_packet {
                                packet.clone()
                            } else {
                                match send_receiver.try_recv() {
                                    Ok(data) => data,
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        panic!("sender disconnected!")
                                    }
                                }
                            };

                            let send_result = match data {
                                UdpSendEvent::Server(ref data, length, addr, _) => {
                                    socket.send_to(&data[..length], addr)
                                }
                                UdpSendEvent::ServerNonTracking(ref data, length, addr) => {
                                    socket.send_to(&data[..length], addr)
                                }
                                UdpSendEvent::Client(ref data, length, _) => {
                                    socket.send(&data[..length])
                                }
                                UdpSendEvent::ClientNonTracking(ref data, length) => {
                                    socket.send(&data[..length])
                                }
                            };

                            match send_result {
                                Ok(length) => {
                                    info!("sent packet of size {length} on {local_addr}");

                                    let event = match data {
                                        UdpSendEvent::Server(data, _, addr, seq) => {
                                            array_pool.free(data);
                                            Some(UdpEvent::SentServer(addr, seq, Instant::now()))
                                        }
                                        UdpSendEvent::Client(data, _, seq) => {
                                            array_pool.free(data);
                                            Some(UdpEvent::SentClient(seq, Instant::now()))
                                        }
                                        UdpSendEvent::ServerNonTracking(data, _, _) => {
                                            array_pool.free(data);
                                            None
                                        }
                                        UdpSendEvent::ClientNonTracking(data, _) => {
                                            array_pool.free(data);
                                            None
                                        }
                                    };

                                    if let Some(event) = event {
                                        event_sender.send(event)?;
                                    }
                                }
                                Err(ref e) if would_block(e) => {
                                    //send would block so we store the last packet and try again
                                    unsent_packet = Some(data);
                                    send_finished = false;
                                    break;
                                }
                                Err(e) => {
                                    //no point of returning the data to the pool.. program crashed anyway..

                                    return Err(e.into());
                                }
                                _ => {}
                            };
                        }

                        //if we sent all of the packets in the channel we can switch back to readable events
                        if send_finished {
                            poll.registry().reregister(
                                &mut socket,
                                UDP_SOCKET,
                                Interest::READABLE,
                            )?;
                        }
                    } else if event.is_readable() {
                        // In this loop we receive all packets queued for the socket.
                        loop {
                            match socket.recv_from(&mut buf) {
                                Ok((packet_size, source_address)) => {
                                    if packet_size >= 4 && buf[..4] == MAGIC_NUMBER_HEADER {
                                        info!(
                                            "received packet of size {packet_size} on {local_addr}"
                                        );
                                        let data_size = packet_size - 4;
                                        let mut pooled_vec = array_pool.rent(data_size);

                                        //copy the data
                                        pooled_vec[..data_size]
                                            .copy_from_slice(&buf[4..packet_size]);

                                        event_sender.send(UdpEvent::Read(
                                            source_address,
                                            data_size,
                                            pooled_vec,
                                        ))?;
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

fn would_block(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock
}
