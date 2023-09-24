use std::time::Instant;

use crossbeam_channel::Sender;

use crate::io::{
    channel::{Channel, ChannelType},
    header::{Header, SendType},
    socket::UdpSendEvent,
};

use super::identity::Identity;

#[derive(Clone)]
pub struct Client {
    pub identity: Identity,
    pub channel: Channel,
    pub received_at: Instant,
    pub last_received: Instant,
}

impl Client {
    pub fn new(identity: Identity, sender: &Sender<UdpSendEvent>) -> Self {
        Self {
            channel: Channel::new(
                identity.addr,
                identity.session_key,
                ChannelType::Server,
                sender,
            ),
            identity,
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }

    pub fn update(&mut self) {
        let resend_packets = self.channel.get_redelivery_packet();
        for packet in resend_packets {
            let mut header = Header::new(
                packet.seq,
                self.identity.session_key,
                SendType::Reliable,
                packet.frag,
            );
            self.channel.write_header_ack_fiels(&mut header);

            let payload = header.create_packet(Some(&packet.data));

            self.channel.resend_reliable(packet.seq, payload);

            self.channel.send_ack = false;
        }

        if self.channel.send_ack {
            self.channel.send_unreliable(None);
        }
    }
}
