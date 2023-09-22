use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use crossbeam_channel::Sender;
use rand::Rng;

use crate::io::{
    channel::Channel,
    header::{Header, SendType},
    socket::{ServerSendPacket, UdpSender},
};

use super::identity::Identity;

#[derive(Clone)]
pub struct Client {
    pub identity: Identity,
    pub channel: Channel<ServerSendPacket>,
    pub received_at: Instant,
    pub last_received: Instant,
}

impl Client {
    pub fn new(identity: Identity, sender: &Sender<ServerSendPacket>) -> Self {
        Self {
            channel: Channel::<ServerSendPacket>::new(identity.addr, sender),
            identity,
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
