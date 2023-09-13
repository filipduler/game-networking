use crossbeam_channel::{bounded, Receiver, SendError, Sender, TryRecvError, TrySendError};
use log::warn;
use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token};
use std::borrow::BorrowMut;
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::ops::Deref;

use crate::io::MAGIC_NUMBER_HEADER;

// A token to allow us to identify which event is for the `UdpSocket`.
const UDP_SOCKET: Token = Token(0);

pub fn run(
    rx: Receiver<(SocketAddr, Vec<u8>)>,
    tx: Sender<(SocketAddr, Vec<u8>)>,
) -> io::Result<()> {
    env_logger::init();

    // Create a poll instance.
    let mut poll = Poll::new()?;
    // Create storage for events. Since we will only register a single socket, a
    // capacity of 1 will do.
    let mut events = Events::with_capacity(1);

    // Setup the UDP socket.
    let addr = "127.0.0.1:9001".parse().unwrap();

    let mut socket = UdpSocket::bind(addr)?;

    // Register our socket with the token defined above and an interest in being
    // `READABLE`.
    poll.registry()
        .register(&mut socket, UDP_SOCKET, Interest::READABLE)?;

    println!("You can connect to the server using `nc`:");
    println!(" $ nc -u 127.0.0.1 9000");
    println!("Anything you type will be echoed back to you.");

    // Initialize a buffer for the UDP packet. We use the maximum size of a UDP
    // packet, which is the maximum value of 16 a bit integer.
    let mut buf = [0; 1 << 16];

    let mut unsent_packet = None;
    let mut outgoing_packets: VecDeque<(SocketAddr, Vec<u8>)> = VecDeque::new();

    // Our event loop.
    loop {
        //check if there are and send requests
        if !rx.is_empty() {
            poll.registry().reregister(
                &mut socket,
                UDP_SOCKET,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        }

        //process all outgoing requests
        if !outgoing_packets.is_empty() {
            loop {
                if tx.is_full() {
                    break;
                }

                if let Some(packet) = outgoing_packets.pop_back() {
                    match tx.try_send(packet) {
                        Err(TrySendError::Full(packet)) => {
                            //this shouldn't happen becase we're checking if the channel is full
                            //requeue the packet and wait
                            outgoing_packets.push_back(packet);
                            break;
                        }
                        Err(TrySendError::Disconnected(_)) => panic!("sender disconnected!"),
                        _ => {}
                    }
                }
            }
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
                    if event.is_writable() {
                        println!("writable!");
                        let mut send_finished = true;

                        loop {
                            let data = if unsent_packet.is_some() {
                                unsent_packet.clone().unwrap()
                            } else {
                                match rx.try_recv() {
                                    Ok(data) => data,
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        panic!("sender disconnected!")
                                    }
                                }
                            };

                            match socket.send_to(&data.1, data.0) {
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    //send would block so we store the last packet and try again
                                    unsent_packet = Some(data);
                                    send_finished = false;
                                    break;
                                }
                                Err(e) => {
                                    return Err(e);
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
                        match socket.recv_from(&mut buf) {
                            Ok((packet_size, source_address)) => {
                                if packet_size >= 4 && buf[..4] == MAGIC_NUMBER_HEADER {
                                    outgoing_packets
                                        .push_front((source_address, buf[..packet_size].to_vec()));
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
