use std::time::Duration;

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use rand::Rng;

use crate::io::{
    int_buffer::IntBuffer,
    socket::{UdpEvent, UdpSendEvent},
    PacketType, MAGIC_NUMBER_HEADER,
};

pub fn try_login(
    reciever: &Receiver<UdpEvent>,
    sender: &Sender<UdpSendEvent>,
) -> anyhow::Result<(u64, u32)> {
    let timeout = Duration::from_secs(5);
    let client_salt = rand::thread_rng().gen();

    //wait for start
    let event = reciever.recv_timeout(timeout)?;
    if event != UdpEvent::Start {
        bail!("expected start event");
    }

    //send connection request
    send_connection_request(client_salt, sender)?;

    //wait for the challange
    let server_salt = read_challange(client_salt, reciever, timeout)?;

    //send the challange response
    send_challange_response(client_salt, server_salt, sender)?;

    //wait for accept or deny response
    let client_id = read_connection_status(reciever, timeout)?;

    Ok((client_salt ^ server_salt, client_id))
}

fn send_connection_request(client_salt: u64, sender: &Sender<UdpSendEvent>) -> anyhow::Result<()> {
    let mut buffer = IntBuffer { index: 0 };

    let mut payload = vec![0_u8; 21];

    buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
    buffer.write_u8(PacketType::ConnectionRequest as u8, &mut payload);
    buffer.write_u64(client_salt, &mut payload);

    sender.send(UdpSendEvent::ClientNonTracking(payload))?;

    Ok(())
}

fn read_challange(
    client_salt: u64,
    reciever: &Receiver<UdpEvent>,
    timeout: Duration,
) -> anyhow::Result<u64> {
    let data = if let UdpEvent::Read(_, data) = reciever.recv_timeout(timeout)? {
        data
    } else {
        bail!("unexpected event");
    };

    let mut buffer = IntBuffer { index: 0 };
    let state = if let Some(state) = PacketType::from_repr(buffer.read_u8(&data)) {
        state
    } else {
        bail!("invalid connection state in header");
    };

    if client_salt != buffer.read_u64(&data) {
        bail!("invalid client salt");
    }
    let server_salt = buffer.read_u64(&data);

    Ok(server_salt)
}

fn read_connection_status(reciever: &Receiver<UdpEvent>, timeout: Duration) -> anyhow::Result<u32> {
    let data = if let UdpEvent::Read(_, data) = reciever.recv_timeout(timeout)? {
        data
    } else {
        bail!("unexpected event");
    };

    let mut buffer = IntBuffer { index: 0 };
    let state = if let Some(state) = PacketType::from_repr(buffer.read_u8(&data)) {
        state
    } else {
        bail!("invalid connection state in header");
    };

    if state == PacketType::ConnectionAccepted {
        return Ok(buffer.read_u32(&data));
    }

    bail!("connection not accepted");
}

fn send_challange_response(
    client_salt: u64,
    server_salt: u64,
    sender: &Sender<UdpSendEvent>,
) -> anyhow::Result<()> {
    let mut buffer = IntBuffer { index: 0 };

    let mut payload = vec![0_u8; 21];
    buffer.index = 0;

    buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
    buffer.write_u8(PacketType::ChallangeResponse as u8, &mut payload);
    buffer.write_u64(client_salt ^ server_salt, &mut payload);

    sender.send(UdpSendEvent::ClientNonTracking(payload))?;

    Ok(())
}
