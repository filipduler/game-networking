use std::{rc::Rc, time::Instant};

use bit_field::BitField;

use super::{sequence_buffer::SequenceBuffer, BUFFER_SIZE};

#[derive(Clone)]
pub struct SendBuffer {
    pub payload: Rc<SendPayload>,
    pub sent_at: Option<Instant>,
    pub created_at: Instant,
}

pub struct SendPayload {
    pub seq: u32,
    pub data: Vec<u8>,
    pub frag: bool,
}

#[derive(Clone)]
pub struct SendBufferManager {
    pub buffers: SequenceBuffer<SendBuffer>,
    pub received_acks: SequenceBuffer<bool>,
}

impl SendBufferManager {
    pub fn new() -> Self {
        SendBufferManager {
            buffers: SequenceBuffer::with_capacity(BUFFER_SIZE),
            received_acks: SequenceBuffer::with_capacity(BUFFER_SIZE),
        }
    }

    pub fn mark_sent(&mut self, seq: u32, sent_at: Instant) {
        if let Some(buffer) = self.buffers.get_mut(seq) {
            buffer.sent_at = Some(sent_at);
        }
    }

    pub fn push_send_buffer(&mut self, seq: u32, data: &[u8], frag: bool) -> Rc<SendPayload> {
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
        self.buffers.remove(ack);
        self.received_acks.insert(ack, true);
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self, remote_seq: u32) -> u32 {
        let mut ack_bitfield = 0;

        //if remote sequence is at 0 or 1 the bitfield will be empty
        if remote_seq > 1 {
            let mut seq = remote_seq - 1;
            for _ in 0..32 {
                if let Some(value) = self.received_acks.get(seq) {
                    if *value {
                        ack_bitfield.set_bit((remote_seq - seq - 1) as usize, true);
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
