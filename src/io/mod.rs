use std::time::Duration;

use strum_macros::FromRepr;

pub mod channel;
pub mod client;
pub mod client_process;
mod connection;
pub mod header;
mod int_buffer;
mod login;
mod send_buffer;
mod sequence_buffer;
pub mod server;
pub mod server_process;
mod socket;

pub const MAGIC_NUMBER_HEADER: [u8; 4] = [1, 27, 25, 14];

pub const BUFFER_SIZE: u32 = 1024;
//always has to be less than BUFFER SIZE
pub const BUFFER_WINDOW_SIZE: u32 = 516;

pub const RESEND_DURATION: Duration = Duration::from_millis(100);
pub const MAX_PACKET_SIZE: usize = 1150;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, FromRepr)]
pub enum PacketType {
    ConnectionRequest = 1,
    Challenge = 2,
    ChallangeResponse = 3,
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
