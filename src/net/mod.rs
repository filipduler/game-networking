use std::time::Duration;

use strum_macros::FromRepr;

//mod array_pool;
pub mod channel;
pub mod client;
pub mod client_process;
mod connections;
mod fragmentation_manager;
pub mod header;
mod int_buffer;
mod packets;
mod rtt_tracker;
mod send_buffer;
mod sequence;
pub mod server;
pub mod server_process;
mod socket;

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
        buffer[..4].copy_from_slice(&MAGIC_NUMBER_HEADER);
        buffer
    }};
}
pub(crate) use {bytes, bytes_with_header};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, FromRepr)]
pub enum PacketType {
    ConnectionRequest = 1,
    Challenge = 2,
    ChallengeResponse = 3,
    ConnectionAccepted = 4,
    PayloadReliableFrag = 5,
    PayloadReliable = 6,
    PayloadUnreliableFrag = 7,
    PayloadUnreliable = 8,
}

impl PacketType {
    pub fn is_frag_variant(&self) -> bool {
        *self == PacketType::PayloadReliableFrag || *self == PacketType::PayloadUnreliableFrag
    }
}
