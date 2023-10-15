use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use anyhow::bail;
use crossbeam_channel::{Receiver, Sender};
use log::warn;
use rand::Rng;

use crate::net::{
    bytes, bytes_with_header,
    int_buffer::IntBuffer,
    socket::{Socket, UdpEvent, UdpSendEvent},
    Bytes, PacketType, MAGIC_NUMBER_HEADER,
};

const REPLY_TIMEOUT: Duration = Duration::from_millis(150);
const RETRIES: usize = 5;

pub struct ConnectionResponse {
    pub session_key: u64,
    pub connection_id: u32,
}

pub struct ConnectionHandshake<'a> {
    socket: &'a mut Socket,
    events: VecDeque<UdpEvent>,
    client_salt: u64,
    server_salt: Option<u64>,
}

impl<'a> ConnectionHandshake<'a> {
    pub fn new(socket: &'a mut Socket) -> ConnectionHandshake {
        ConnectionHandshake {
            socket,
            events: VecDeque::with_capacity(1),
            client_salt: rand::thread_rng().gen(),
            server_salt: None,
        }
    }

    pub fn try_login(&mut self) -> anyhow::Result<ConnectionResponse> {
        for _ in 0..RETRIES {
            self.server_salt = None;

            for _ in 0..RETRIES {
                //send connection request
                self.send_connection_request();

                //wait for the challenge
                match self.read_challenge() {
                    Ok(server_salt) => {
                        self.server_salt = Some(server_salt);
                        break;
                    }
                    Err(e) => {
                        warn!("failed reading connection challenge: {e}");
                    }
                }
            }

            if let Some(server_salt) = self.server_salt {
                for _ in 0..RETRIES {
                    //send the challenge response
                    self.send_challenge_response(server_salt);

                    //wait for accept or deny response
                    match self.read_connection_status() {
                        Ok(connection_id) => {
                            return Ok(ConnectionResponse {
                                session_key: self.client_salt ^ server_salt,
                                connection_id,
                            });
                        }
                        Err(e) => {
                            warn!("failed reading connection challenge response: {e}");
                        }
                    }
                }
            }
        }

        bail!("failed connecting to server");
    }

    fn send_connection_request(&mut self) {
        let mut int_buffer = IntBuffer::new_at(4);

        let mut buffer = bytes_with_header!(9);

        int_buffer.write_u8(PacketType::ConnectionRequest as u8, &mut buffer);
        int_buffer.write_u64(self.client_salt, &mut buffer);

        self.socket.enqueue_send_event(UdpSendEvent::Client(buffer));
    }

    fn read_challenge(&mut self) -> anyhow::Result<u64> {
        let buffer: Vec<u8> = self.read_udp_event()?;

        let mut int_buffer = IntBuffer::default();
        let state = PacketType::try_from(int_buffer.read_u8(&buffer))?;

        if self.client_salt != int_buffer.read_u64(&buffer) {
            bail!("invalid client salt");
        }
        let server_salt = int_buffer.read_u64(&buffer);

        Ok(server_salt)
    }

    fn read_connection_status(&mut self) -> anyhow::Result<u32> {
        let buffer: Vec<u8> = self.read_udp_event()?;

        let mut int_buffer = IntBuffer::default();
        let state = PacketType::try_from(int_buffer.read_u8(&buffer))?;

        if state == PacketType::ConnectionAccepted {
            return Ok(int_buffer.read_u32(&buffer));
        }

        bail!("connection not accepted");
    }

    fn send_challenge_response(&mut self, server_salt: u64) {
        let mut int_buffer = IntBuffer::new_at(4);

        let mut buffer = bytes_with_header!(9);

        int_buffer.write_u8(PacketType::ChallengeResponse as u8, &mut buffer);
        int_buffer.write_u64(self.client_salt ^ server_salt, &mut buffer);

        self.socket.enqueue_send_event(UdpSendEvent::Client(buffer));
    }

    fn read_udp_event(&mut self) -> anyhow::Result<Bytes> {
        self.events.clear();

        self.socket
            .process(Instant::now() + REPLY_TIMEOUT, Some(1), &mut self.events)?;

        if let Some(UdpEvent::Read(_, buffer, _)) = self.events.pop_back() {
            return Ok(buffer);
        }

        bail!("expected read event")
    }
}
