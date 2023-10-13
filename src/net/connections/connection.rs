use std::{collections::VecDeque, rc::Rc, sync::Arc, time::Instant};

use crossbeam_channel::Sender;
use log::error;

use crate::net::{
    channel::{Channel, ChannelType},
    header::{Header, SendType},
    send_buffer::SendPayload,
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

    pub fn update(
        &mut self,
        marked_packets: &mut Vec<Rc<SendPayload>>,
        send_queue: &mut VecDeque<UdpSendEvent>,
    ) {
        if let Err(e) = self.channel.update(marked_packets, send_queue) {
            error!("error updating channel: {e}");
        }
    }
}
