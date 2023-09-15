use std::{net::SocketAddr, time::Instant};

use rand::Rng;

use super::channel::Channel;

pub struct Connection {
    pub identity: Identity,
    pub reliable_channel: Channel,
    pub received_at: Instant,
    pub last_received: Instant,
}

impl Connection {
    pub fn new(addr: SocketAddr, client_salt: u64) -> Self {
        let server_salt = rand::thread_rng().gen();

        Self {
            identity: Identity {
                addr,
                client_salt,
                server_salt,
                session_key: client_salt ^ server_salt,
                accepted: false,
            },
            reliable_channel: Channel {
                local_seq: 0,
                remote_seq: 0,
                ack_bits: 0,
            },
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }
}

pub struct Identity {
    pub addr: SocketAddr,
    pub client_salt: u64,
    pub server_salt: u64,
    pub session_key: u64,
    pub accepted: bool,
}
