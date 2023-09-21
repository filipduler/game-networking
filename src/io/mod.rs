use std::time::Duration;

pub mod channel;
pub mod client;
pub mod client_process;
mod connection;
pub mod header;
mod int_buffer;
mod send_buffer;
mod sequence_buffer;
pub mod server;
pub mod server_process;
mod socket;

pub const BUFFER_SIZE: u32 = 1024;
pub const MAGIC_NUMBER_HEADER: [u8; 4] = [1, 27, 25, 14];
pub const RESENT_DURATION: Duration = Duration::from_millis(100);

#[repr(u8)]
pub enum MessageType {
    ConnectionRequest = 1,
    ConnectionDenied = 2,
    Challange = 3,
    ChallangeResponse = 4,
    ConnectionPayload = 5,
    Disconnect = 6,
}
