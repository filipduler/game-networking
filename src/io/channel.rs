use std::rc::Rc;

use super::{
    header::{Header, HEADER_SIZE},
    send_buffer::{SendBufferManager, SendPayload},
    sequence_buffer::SequenceBuffer,
    BUFFER_SIZE, MAGIC_NUMBER_HEADER,
};
use bit_field::BitField;

pub struct Channel {
    pub local_seq: u32,
    pub remote_seq: u32,
    pub send_ack: bool,
    //buffer of sent packets
    pub send_buffer: SendBufferManager,
    //acks of sent packets
    pub received_acks: SequenceBuffer<bool>,
}

impl Channel {
    pub fn new() -> Self {
        Self {
            local_seq: 0,
            remote_seq: 0,
            send_ack: false,
            send_buffer: SendBufferManager::new(),
            received_acks: SequenceBuffer::with_capacity(BUFFER_SIZE),
        }
    }

    pub fn create_send_buffer(&mut self, data: Option<&[u8]>) -> Rc<SendPayload> {
        let data_len = if let Some(d) = data { d.len() } else { 0 };

        let mut payload = vec![0_u8; data_len + HEADER_SIZE + 4];
        payload[..4].copy_from_slice(&MAGIC_NUMBER_HEADER);

        let header = Header {
            seq: self.local_seq,
            ack: self.remote_seq,
            ack_bits: self.generate_ack_field(),
        };
        header.write(&mut payload[4..]);
        if let Some(d) = data {
            payload[HEADER_SIZE + 4..].copy_from_slice(d);
        }

        let send_payload = self.send_buffer.push_send_buffer(self.local_seq, &payload);

        self.local_seq += 1;
        self.send_ack = false;

        send_payload
    }

    pub fn update_remote_seq(&mut self, remote_seq: u32) {
        if remote_seq > self.remote_seq {
            self.remote_seq = remote_seq;
        }
    }

    pub fn mark_sent_packets(&mut self, ack: u32, ack_bitfield: u32) {
        self.ack_packet(ack);

        for bit_pos in 0..32 {
            if ack_bitfield.get_bit(bit_pos) {
                let seq = ack - bit_pos as u32 - 1;
                self.ack_packet(seq);
            }
        }
    }

    fn ack_packet(&mut self, ack: u32) {
        self.send_buffer.buffers.remove(ack);
        self.received_acks.insert(ack, true);
    }

    //least significant bit is the remote_seq - 1 value
    fn generate_ack_field(&self) -> u32 {
        let mut ack_bitfield = 0;

        //if remote sequence is at 0 or 1 the bitfield will be empty
        if self.remote_seq > 1 {
            let mut seq = self.remote_seq - 1;
            for _ in 0..32 {
                if let Some(value) = self.received_acks.get(seq) {
                    if *value {
                        ack_bitfield.set_bit((self.remote_seq - seq - 1) as usize, true);
                    }
                }
                if seq == 0 {
                    break;
                }
                seq -= 1
            }
        }

        ack_bitfield
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

        assert!(*channel.received_acks.get(50).unwrap());
        assert!(*channel.received_acks.get(49).unwrap());
        assert!(*channel.received_acks.get(48).unwrap());
        assert!(*channel.received_acks.get(34).unwrap());
        assert!(*channel.received_acks.get(18).unwrap());
    }

    #[test]
    fn generating_received_bitfields() {
        let mut channel = Channel::new();
        channel.local_seq = 50;
        channel.remote_seq = 70;

        let prev_remote_seq = channel.remote_seq - 1;
        channel.received_acks.insert(prev_remote_seq, true);
        channel.received_acks.insert(prev_remote_seq - 1, true);
        channel.received_acks.insert(prev_remote_seq - 15, true);
        channel.received_acks.insert(prev_remote_seq - 31, true);

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(channel.generate_ack_field(), ack_bitfield);
    }
}
