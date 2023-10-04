use std::{collections::VecDeque, sync::Arc, time::Instant};

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
}

impl Connection {
    pub fn new(identity: Identity) -> Self {
        Self {
            channel: Channel::new(identity.addr, identity.session_key, ChannelType::Server),
            identity,
            received_at: Instant::now(),
            last_received: Instant::now(),
        }
    }

    pub fn update(&mut self, send_queue: &mut VecDeque<UdpSendEvent>) {
        let resend_packets = self.channel.get_redelivery_packet();
        for packet in resend_packets {
            let mut header = Header::new(
                packet.seq,
                self.identity.session_key,
                SendType::Reliable,
                packet.frag,
            );
            self.channel.write_header_ack_fiels(&mut header);

            //let buffer = header.create_packet(Some(&packet.buffer.used_data()));

            //self.channel.send(packet.seq, buffer, send_queue);
        }

        if self.channel.send_ack {
            self.channel.send_empty_ack(send_queue);
        }
    }
}
