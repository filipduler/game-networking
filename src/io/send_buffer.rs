use std::{rc::Rc, time::Instant};

use bit_field::BitField;

use crate::io::{sequence::SequenceBuffer, BUFFER_SIZE};

pub struct SendBuffer {
    pub payload: Rc<SendPayload>,
    pub sent_at: Option<Instant>,
    pub created_at: Instant,
}

pub struct SendPayload {
    pub seq: u16,
    pub data: Vec<u8>,
    pub frag: bool,
}

pub struct SendBufferManager {
    pub buffers: SequenceBuffer<SendBuffer>,
    pub received_acks: SequenceBuffer<bool>,
}

impl SendBufferManager {
    pub fn new() -> Self {
        SendBufferManager {
            buffers: SequenceBuffer::with_size(BUFFER_SIZE),
            received_acks: SequenceBuffer::with_size(BUFFER_SIZE),
        }
    }

    pub fn mark_sent(&mut self, seq: u16, sent_at: Instant) {
        if let Some(buffer) = self.buffers.get_mut(seq) {
            buffer.sent_at = Some(sent_at);
        }
    }

    pub fn push_send_buffer(&mut self, seq: u16, data: &[u8], frag: bool) -> Rc<SendPayload> {
        let send_buffer = SendBuffer {
            payload: Rc::new(SendPayload {
                seq,
                data: data.to_vec(),
                frag,
            }),
            sent_at: None,
            created_at: Instant::now(),
        };

        let payload = send_buffer.payload.clone();

        self.buffers.insert(seq, send_buffer);

        payload
    }

    pub fn mark_sent_packets(&mut self, ack: u16, ack_bitfield: u32) {
        self.ack_packet(ack);

        for bit_pos in 0..32_u16 {
            if ack_bitfield.get_bit(bit_pos as usize) {
                let seq = ack.wrapping_sub(bit_pos).wrapping_sub(1);
                self.ack_packet(seq);
            }
        }
    }

    fn ack_packet(&mut self, ack: u16) {
        self.buffers.remove(ack);
        self.received_acks.insert(ack, true);
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self, remote_seq: u16) -> u32 {
        let mut ack_bitfield = 0;

        let mut seq = remote_seq.wrapping_sub(1);
        for pos in 0..32 {
            if let Some(value) = self.received_acks.get(seq) {
                if *value {
                    ack_bitfield.set_bit(pos, true);
                }
            }
            seq = seq.wrapping_sub(1);
        }

        ack_bitfield
    }
}
