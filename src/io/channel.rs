use super::send_buffer::SendBufferManager;

pub struct Channel {
    pub local_seq: u32,
    pub remote_seq: u32,
    pub ack_bits: u32,
    pub send_buffer: SendBufferManager,
}

impl Channel {
    pub fn new() -> Self {
        Self {
            local_seq: 0,
            remote_seq: 0,
            ack_bits: 0,
            send_buffer: SendBufferManager::new(),
        }
    }
}
