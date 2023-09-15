pub mod channel;
mod connection;
mod header;
pub mod inner_server;
mod int_buffer;
pub mod server;
pub mod udp;
const MAGIC_NUMBER_HEADER: [u8; 4] = [1, 27, 25, 14];
