use std::{sync::Arc, time::Instant};

use crossbeam_channel::Sender;

use crate::net::{
    array_pool::ArrayPool,
    channel::{Channel, ChannelType},
    header::{Header, SendType},
    socket::UdpSendEvent,
};

use super::identity::Identity;

pub struct Connection {
    pub identity: Identity,
    pub channel: Channel,
    pub received_at: Instant,
    pub last_received: Instant,
    pool_array: Arc<ArrayPool>,
}

impl Connection {
    pub fn new(
        identity: Identity,
        sender: &Sender<UdpSendEvent>,
        pool_array: &Arc<ArrayPool>,
    ) -> Self {
        Self {
            channel: Channel::new(
                identity.addr,
                identity.session_key,
                ChannelType::Server,
                sender,
                pool_array,
            ),
            identity,
            received_at: Instant::now(),
            last_received: Instant::now(),
            pool_array: pool_array.clone(),
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

            let (payload, length) = header.create_packet(Some(&packet.data), &self.pool_array);

            self.channel.send(packet.seq, payload, length);
        }

        if self.channel.send_ack {
            self.channel.send_empty_ack();
        }
    }
}
