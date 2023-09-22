use std::{net::SocketAddr, rc::Rc};

use crossbeam_channel::Sender;

use super::{
    header::{Header, SendType},
    send_buffer::{SendBufferManager, SendPayload},
    socket::UdpSendEvent,
    RESENT_DURATION,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Client,
    Server,
}

#[derive(Clone)]
pub struct Channel {
    pub mode: ChannelType,
    pub session_key: u64,
    pub addr: SocketAddr,
    pub unreliable_local_seq: u32,
    pub local_seq: u32,
    pub remote_seq: u32,
    pub send_ack: bool,
    //buffer of sent packets
    pub send_buffer: SendBufferManager,
    sender: Sender<UdpSendEvent>,
}

impl Channel {
    pub fn new(
        addr: SocketAddr,
        session_key: u64,
        mode: ChannelType,
        sender: &Sender<UdpSendEvent>,
    ) -> Self {
        Self {
            mode,
            session_key,
            addr,
            unreliable_local_seq: 0,
            local_seq: 0,
            remote_seq: 0,
            send_ack: false,
            send_buffer: SendBufferManager::new(),
            sender: sender.clone(),
        }
    }

    pub fn resend_reliable(&mut self, seq: u32, payload: Vec<u8>) {
        self.sender
            .send(self.make_send_event(seq, payload))
            .unwrap();
        self.send_ack = false;
    }

    pub fn send_reliable(&mut self, data: Option<&[u8]>) {
        let send_buffer = self.create_send_buffer(data);
        self.sender
            .send(self.make_send_event(send_buffer.seq, send_buffer.data.to_vec()))
            .unwrap();
        self.send_ack = false;
    }

    pub fn send_unreliable(&mut self, data: Option<&[u8]>) {
        let mut header = Header::new(
            self.unreliable_local_seq,
            self.session_key,
            SendType::Unreliable,
        );
        self.write_header_ack_fiels(&mut header);

        let payload = Header::create_packet(&header, data);

        self.unreliable_local_seq += 1;
        self.send_ack = false;

        self.sender
            .send(self.make_send_event(header.seq, payload))
            .unwrap();
    }

    pub fn read(&mut self, data: &[u8]) {
        let header = Header::read(data);
        //TODO: check if its duplicate

        self.update_remote_seq(header.seq);
        //mark messages as sent
        self.mark_sent_packets(header.ack, header.ack_bits);

        //send ack
        self.send_ack = true;
    }

    pub fn write_header_ack_fiels(&self, header: &mut Header) {
        header.ack = self.remote_seq;
        header.ack_bits = self.generate_ack_field();
    }

    pub fn create_send_buffer(&mut self, data: Option<&[u8]>) -> Rc<SendPayload> {
        let mut header = Header::new(self.local_seq, self.session_key, SendType::Reliable);
        self.write_header_ack_fiels(&mut header);

        let payload = Header::create_packet(&header, data);
        let send_payload = self.send_buffer.push_send_buffer(self.local_seq, &payload);

        self.send_ack = false;
        self.local_seq += 1;

        send_payload
    }

    pub fn get_redelivery_packet(&mut self) -> Vec<Rc<SendPayload>> {
        let mut packets = Vec::new();
        if self.local_seq > 0 {
            let mut current_seq = self.local_seq - 1;

            while let Some(send_buffer) = self.send_buffer.buffers.get_mut(current_seq) {
                if let Some(sent_at) = send_buffer.sent_at {
                    if sent_at.elapsed() > RESENT_DURATION {
                        packets.push(send_buffer.payload.clone());
                        send_buffer.sent_at = None;
                    }
                }
                if current_seq > 0 {
                    current_seq -= 1;
                } else {
                    break;
                }
            }
        }

        packets
    }

    pub fn update_remote_seq(&mut self, remote_seq: u32) {
        if remote_seq > self.remote_seq {
            self.remote_seq = remote_seq;
        }
    }

    pub fn mark_sent_packets(&mut self, ack: u32, ack_bitfield: u32) {
        self.send_buffer.mark_sent_packets(ack, ack_bitfield)
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self) -> u32 {
        self.send_buffer.generate_ack_field(self.remote_seq)
    }

    fn make_send_event(&self, seq: u32, payload: Vec<u8>) -> UdpSendEvent {
        match self.mode {
            ChannelType::Client => UdpSendEvent::ClientTracking(payload, seq),
            ChannelType::Server => UdpSendEvent::ServerTracking(payload, self.addr, seq),
        }
    }
}

#[cfg(test)]
mod tests {
    use bit_field::BitField;

    fn test_channel() -> Channel {
        let (t1, t2) = crossbeam_channel::unbounded();
        Channel::new(
            "127.0.0.1:21344".parse().unwrap(),
            0, //TODO
            ChannelType::Server,
            &t1,
        )
    }

    use super::*;
    #[test]
    fn marking_received_bitfields() {
        let mut channel = test_channel();
        channel.local_seq = 50;
        channel.remote_seq = 70;

        //WARN: if seq ever gets converted to u16 we need to test for appropriately for it
        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        channel.mark_sent_packets(50, ack_bitfield);

        assert!(*channel.send_buffer.received_acks.get(50).unwrap());
        assert!(*channel.send_buffer.received_acks.get(49).unwrap());
        assert!(*channel.send_buffer.received_acks.get(48).unwrap());
        assert!(*channel.send_buffer.received_acks.get(34).unwrap());
        assert!(*channel.send_buffer.received_acks.get(18).unwrap());
    }

    #[test]
    fn generating_received_bitfields() {
        let mut channel = test_channel();
        channel.local_seq = 50;
        channel.remote_seq = 70;

        let prev_remote_seq = channel.remote_seq - 1;
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 1, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 15, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 31, true);

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(channel.generate_ack_field(), ack_bitfield);
    }
}
