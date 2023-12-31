use std::time::Duration;

use anyhow::bail;

//mod array_pool;
mod channel;
mod client;
mod client_process;
mod connections;
mod fragmentation_manager;
mod header;
mod int_buffer;
mod packets;
mod rtt_tracker;
mod send_buffer;
mod sequence;
mod server;
mod server_process;
mod socket;

pub use client::Client;
pub use fragmentation_manager::{FRAGMENT_SIZE, MAX_FRAGMENT_SIZE};
pub use header::SendType;
pub use server::{Server, ServerEvent};

pub const MAGIC_NUMBER_HEADER: [u8; 4] = [1, 27, 25, 14];
pub const BUFFER_SIZE: u16 = 1024;
//always has to be less than BUFFER SIZE
pub const BUFFER_WINDOW_SIZE: u16 = 256;

pub type Bytes = Vec<u8>;
macro_rules! bytes {
    ($size:expr) => {{
        vec![0_u8; $size]
    }};
}

macro_rules! bytes_with_header {
    ($payload_size:expr) => {{
        let mut buffer = vec![0_u8; $payload_size + 4];
        buffer[..4].copy_from_slice(&crate::net::MAGIC_NUMBER_HEADER);
        buffer
    }};
}
pub(crate) use {bytes, bytes_with_header};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    ConnectionRequest = 1,
    Challenge = 2,
    ChallengeResponse = 3,
    ConnectionAccepted = 4,
    PayloadReliableFrag = 5,
    PayloadReliable = 6,
    PayloadUnreliableFrag = 7,
    PayloadUnreliable = 8,
    Disconnect = 9,
}

impl PacketType {
    pub fn is_frag_variant(&self) -> bool {
        *self == PacketType::PayloadReliableFrag || *self == PacketType::PayloadUnreliableFrag
    }
}
impl TryFrom<u8> for PacketType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PacketType::ConnectionRequest),
            2 => Ok(PacketType::Challenge),
            3 => Ok(PacketType::ChallengeResponse),
            4 => Ok(PacketType::ConnectionAccepted),
            5 => Ok(PacketType::PayloadReliableFrag),
            6 => Ok(PacketType::PayloadReliable),
            7 => Ok(PacketType::PayloadUnreliableFrag),
            8 => Ok(PacketType::PayloadUnreliable),
            9 => Ok(PacketType::Disconnect),
            _ => bail!("couldn't parse value '{value}' to packet type"),
        }
    }
}
