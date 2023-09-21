use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use crossbeam_channel::Sender;
use rand::Rng;

use super::{
    channel::Channel,
    header::{Header, SendType},
    socket::{ServerSendPacket, UdpSender},
};
static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub struct Connection {
    pub identity: Identity,
    pub channel: Channel<ServerSendPacket>,
    pub received_at: Instant,
    pub last_received: Instant,
}

impl Connection {
    pub fn new(addr: SocketAddr, sender: &Sender<ServerSendPacket>, client_salt: u64) -> Self {
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
            channel: Channel::<ServerSendPacket>::new(addr, sender),
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }

    pub fn update(&mut self) {
        let resend_packets = self.channel.get_redelivery_packet();
        for packet in resend_packets {
            let mut header = Header::new(packet.seq, SendType::Reliable);
            self.channel.write_header_ack_fiels(&mut header);

            let payload = Header::create_packet(&header, Some(&packet.data));

            self.channel.resend_reliable(packet.seq, payload);

            self.channel.send_ack = false;
        }

        if self.channel.send_ack {
            self.channel.send_unreliable(None);
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
