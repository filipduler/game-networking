mod client;
mod identity;
mod manager;

pub use client::Client;
pub use manager::ConnectionManager;
use strum_macros::FromRepr;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, FromRepr)]
pub enum ConnectionState {
    ConnectionRequest = 1,
    Challenge = 2,
    ChallangeResponse = 3,
    ConnectionAccepted = 4,
}
