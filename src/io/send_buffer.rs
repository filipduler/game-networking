use std::{rc::Rc, time::Instant};

use super::{sequence_buffer::SequenceBuffer, BUFFER_SIZE};

pub struct SendBuffer {
    pub seq: u32,
    pub data: Vec<u8>,
    pub created_at: Instant,
}

pub struct SendBufferManager {
    pub buffers: SequenceBuffer<Rc<SendBuffer>>,
}

impl SendBufferManager {
    pub fn new() -> Self {
        SendBufferManager {
            buffers: SequenceBuffer::with_capacity(BUFFER_SIZE),
        }
    }

    pub fn push_send_buffer(&mut self, seq: u32, data: &[u8]) -> Rc<SendBuffer> {
        let send_buffer = Rc::new(SendBuffer {
            data: data.to_vec(),
            seq,
            created_at: Instant::now(),
        });

        self.buffers.insert(seq, send_buffer.clone());
        send_buffer
    }

    /*pub fn get_send_buffer(&mut self, sequence: u16) -> Option<&mut SendBuffer> {
        match self.buffers.get_mut(sequence) {
            Some(send_buffer) => {
                return Some(send_buffer);
            }
            None => {
                return None;
            }
        }
    }

    pub fn expire(&mut self) {
        let mut expired: Vec<u16> = Vec::new();

        for value in &self.buffers.values {
            if let Some(buffer) = value {
                if buffer.created_at.elapsed().as_millis() > EXPIRE {
                    expired.push(buffer.sequence);
                }
            }
        }
        for sequence in expired {
            self.buffers.remove(sequence);
        }
    }

    pub fn create_send_buffer(&mut self, length: usize) -> Option<&mut SendBuffer> {
        self.current_sequence = Sequence::next_sequence(self.current_sequence);

        if let Some(mut send_buffer) = self.buffers.take(self.current_sequence) {
            if send_buffer.byte_buffer.pooled && length <= self.buffer_pool.buffer_size {
                send_buffer.byte_buffer.length = length;
            } else {
                self.buffer_pool.return_buffer(send_buffer.byte_buffer);
                send_buffer.byte_buffer = self.buffer_pool.get_buffer(length);
            }
            send_buffer.sequence = self.current_sequence;
            send_buffer.created_at = Instant::now();
            return self.buffers.insert(self.current_sequence, send_buffer);
        }

        let byte_buffer = self.buffer_pool.get_buffer(length);
        let send_buffer = SendBuffer {
            sequence: self.current_sequence,
            byte_buffer,
            created_at: Instant::now(),
        };
        return self.buffers.insert(self.current_sequence, send_buffer);

    }*/
}
