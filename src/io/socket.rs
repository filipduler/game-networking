use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{info, warn};
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::io::MAGIC_NUMBER_HEADER;

pub enum UdpEvent {
    Start,
    Sent(SocketAddr, u32, Instant),
    Read(SocketAddr, Vec<u8>),
}

// A token to allow us to identify which event is for the `UdpSocket`.
const UDP_SOCKET: Token = Token(0);

pub fn run_udp_socket(
    local_addr: SocketAddr,
    remote_addr_opt: Option<SocketAddr>,
    send_receiver: Receiver<(SocketAddr, u32, Vec<u8>)>,
    event_sender: Sender<UdpEvent>,
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

    let mut unsent_packet = None;
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
                            let data = if unsent_packet.is_some() {
                                unsent_packet.clone().unwrap()
                            } else {
                                match send_receiver.try_recv() {
                                    Ok(data) => data,
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        panic!("sender disconnected!")
                                    }
                                }
                            };

                            let send_result = if client_mode {
                                socket.send(&data.2)
                            } else {
                                socket.send_to(&data.2, data.0)
                            };

                            match send_result {
                                Ok(_) => {
                                    event_sender.send(UdpEvent::Sent(
                                        data.0,
                                        data.1,
                                        Instant::now(),
                                    ))?;
                                }
                                Err(ref e) if would_block(e) => {
                                    //send would block so we store the last packet and try again
                                    unsent_packet = Some(data);
                                    send_finished = false;
                                    break;
                                }
                                Err(e) => {
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
                                        event_sender.send(UdpEvent::Read(
                                            source_address,
                                            buf[4..packet_size].to_vec(),
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
