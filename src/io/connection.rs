use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use crossbeam_channel::Sender;
use rand::Rng;

use super::{
    channel::Channel,
    header::{Header, SendType, HEADER_SIZE},
    send_buffer::SendBufferManager,
    sequence_buffer::SequenceBuffer,
    BUFFER_SIZE, MAGIC_NUMBER_HEADER,
};
static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub struct Connection {
    pub identity: Identity,
    pub reliable_channel: Channel,
    pub received_at: Instant,
    pub last_received: Instant,
}

impl Connection {
    pub fn new(
        addr: SocketAddr,
        sender: &Sender<(SocketAddr, u32, Vec<u8>)>,
        client_salt: u64,
    ) -> Self {
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
            reliable_channel: Channel::new(addr, sender),
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }

    pub fn update(&mut self) {
        let resend_packets = self.reliable_channel.get_redelivery_packet();
        for packet in resend_packets {
            let mut header = Header::new(packet.seq, SendType::Reliable);
            self.reliable_channel.write_header_ack_fiels(&mut header);

            let payload = Header::create_packet(&header, Some(&packet.data));

            self.reliable_channel.resend_reliable(packet.seq, payload);

            self.reliable_channel.send_ack = false;
        }

        if self.reliable_channel.send_ack {
            self.reliable_channel.send_unreliable(None);
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
