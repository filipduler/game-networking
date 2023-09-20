use std::time::Duration;

pub mod channel;
pub mod client;
mod connection;
pub mod header;
pub mod inner_server;
mod int_buffer;
mod send_buffer;
mod sequence_buffer;
pub mod server;
mod socket;
pub const BUFFER_SIZE: u32 = 1024;
pub const MAGIC_NUMBER_HEADER: [u8; 4] = [1, 27, 25, 14];
pub const RESENT_DURATION: Duration = Duration::from_millis(100);
