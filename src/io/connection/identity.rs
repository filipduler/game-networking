use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
};

use rand::Rng;

use super::ConnectionState;
static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Clone)]
pub struct Identity {
    pub id: u32,
    pub addr: SocketAddr,
    pub client_salt: u64,
    pub server_salt: u64,
    pub session_key: u64,
    pub state: ConnectionState,
}

impl Identity {
    pub fn new(addr: SocketAddr, client_salt: u64, state: ConnectionState) -> Self {
        let server_salt = rand::thread_rng().gen();

        Self {
            id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            addr,
            client_salt,
            server_salt,
            session_key: client_salt ^ server_salt,
            state,
        }
    }
}
