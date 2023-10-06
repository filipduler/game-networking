use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use rand::Rng;

use crate::net::{
    array_pool::ArrayPool,
    int_buffer::IntBuffer,
    socket::{Socket, UdpEvent, UdpSendEvent},
    PacketType, MAGIC_NUMBER_HEADER,
};

fn read_udp_event(socket: &mut Socket, timeout: Duration) -> anyhow::Result<UdpEvent> {
    let mut events = VecDeque::with_capacity(1);
    socket.process(Instant::now() + timeout, Some(1), &mut events)?;

    if let Some(event) = events.pop_back() {
        Ok(event)
    } else {
        bail!("no events received");
    }
}

pub fn try_login(socket: &mut Socket) -> anyhow::Result<(u64, u32)> {
    let timeout = Duration::from_secs(5);
    let client_salt = rand::thread_rng().gen();

    //send connection request
    send_connection_request(client_salt, socket)?;

    //wait for the challenge
    let server_salt = read_challenge(client_salt, socket, timeout)?;

    //send the challenge response
    send_challenge_response(client_salt, server_salt, socket)?;

    //wait for accept or deny response
    let client_id = read_connection_status(socket, timeout)?;

    Ok((client_salt ^ server_salt, client_id))
}

fn send_connection_request(client_salt: u64, socket: &mut Socket) -> anyhow::Result<()> {
    let mut int_buffer = IntBuffer::default();

    let mut buffer = ArrayPool::rent(13);

    int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
    int_buffer.write_u8(PacketType::ConnectionRequest as u8, &mut buffer);
    int_buffer.write_u64(client_salt, &mut buffer);

    socket.enqueue_send_event(UdpSendEvent::Client(buffer));

    Ok(())
}

fn read_challenge(client_salt: u64, socket: &mut Socket, timeout: Duration) -> anyhow::Result<u64> {
    let buffer = if let UdpEvent::Read(_, buffer, _) = read_udp_event(socket, timeout)? {
        buffer
    } else {
        bail!("unexpected event");
    };

    let mut int_buffer = IntBuffer { index: 0 };
    let state = if let Some(state) = PacketType::from_repr(int_buffer.read_u8(&buffer)) {
        state
    } else {
        bail!("invalid connection state in header");
    };

    if client_salt != int_buffer.read_u64(&buffer) {
        bail!("invalid client salt");
    }
    let server_salt = int_buffer.read_u64(&buffer);

    Ok(server_salt)
}

fn read_connection_status(socket: &mut Socket, timeout: Duration) -> anyhow::Result<u32> {
    let buffer = if let UdpEvent::Read(_, buffer, _) = read_udp_event(socket, timeout)? {
        buffer
    } else {
        bail!("unexpected event");
    };

    let mut int_buffer = IntBuffer { index: 0 };
    let state = if let Some(state) = PacketType::from_repr(int_buffer.read_u8(&buffer)) {
        state
    } else {
        bail!("invalid connection state in header");
    };

    if state == PacketType::ConnectionAccepted {
        return Ok(int_buffer.read_u32(&buffer));
    }

    bail!("connection not accepted");
}

fn send_challenge_response(
    client_salt: u64,
    server_salt: u64,
    socket: &mut Socket,
) -> anyhow::Result<()> {
    let mut int_buffer = IntBuffer { index: 0 };

    let mut buffer = ArrayPool::rent(21);

    int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
    int_buffer.write_u8(PacketType::ChallengeResponse as u8, &mut buffer);
    int_buffer.write_u64(client_salt ^ server_salt, &mut buffer);

    socket.enqueue_send_event(UdpSendEvent::Client(buffer));

    Ok(())
}
