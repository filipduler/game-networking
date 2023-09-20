use std::{net::SocketAddr, rc::Rc};

use super::{
    header::{Header, SendType, HEADER_SIZE},
    send_buffer::{SendBufferManager, SendPayload},
    sequence_buffer::SequenceBuffer,
    BUFFER_SIZE, MAGIC_NUMBER_HEADER, RESENT_DURATION,
};
use bit_field::BitField;
use crossbeam_channel::Sender;

pub struct Channel {
    pub local_seq: u32,
    pub remote_seq: u32,
    pub send_ack: bool,
    //buffer of sent packets
    pub send_buffer: SendBufferManager,
}

impl Channel {
    pub fn new() -> Self {
        Self {
            local_seq: 0,
            remote_seq: 0,
            send_ack: false,
            send_buffer: SendBufferManager::new(),
        }
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
        let mut header = Header::new(self.local_seq, SendType::Reliable);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marking_received_bitfields() {
        let mut channel = Channel::new();
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
        let mut channel = Channel::new();
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
