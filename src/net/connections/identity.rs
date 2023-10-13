use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use rand::Rng;

static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Clone)]
pub struct Identity {
    pub connection_id: u32,
    pub addr: SocketAddr,
    pub client_salt: u64,
    pub server_salt: u64,
    pub session_key: u64,
    pub created_at: Instant,
}

impl Identity {
    pub fn new(addr: SocketAddr, client_salt: u64) -> Self {
        let server_salt = rand::thread_rng().gen();

        Self {
            connection_id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            addr,
            client_salt,
            server_salt,
            session_key: client_salt ^ server_salt,
            created_at: Instant::now(),
        }
    }
}
