use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use rand::Rng;

use super::{
    channel::Channel, send_buffer::SendBufferManager, sequence_buffer::SequenceBuffer, BUFFER_SIZE,
};
const CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

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
                id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                addr,
                client_salt,
                server_salt,
                session_key: client_salt ^ server_salt,
                accepted: false,
            },
            reliable_channel: Channel::new(),
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }
}

pub struct Identity {
    pub id: u32,
    pub addr: SocketAddr,
    pub client_salt: u64,
    pub server_salt: u64,
    pub session_key: u64,
    pub accepted: bool,
}
