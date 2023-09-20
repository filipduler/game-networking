use strum_macros::FromRepr;

use super::{int_buffer::IntBuffer, MAGIC_NUMBER_HEADER};

pub const HEADER_SIZE: usize = 13;

#[repr(u8)]
#[derive(Clone, Copy, FromRepr)]
pub enum SendType {
    Reliable = 1,
    Unreliable = 2,
}

pub struct Header {
    pub seq: u32,
    pub message_type: SendType,
    pub ack: u32,
    pub ack_bits: u32,
}

impl Header {
    pub fn new(seq: u32, message_type: SendType) -> Self {
        Self {
            seq,
            message_type,
            ack: 0,
            ack_bits: 0,
        }
    }

    pub fn write(&self, data: &mut [u8]) {
        assert!(
            data.len() >= HEADER_SIZE,
            "data length needs to be atleast bytes {HEADER_SIZE} long."
        );

        //TODO: check data length!
        let mut buffer = IntBuffer { index: 0 };
        buffer.write_u32(self.seq, data);
        buffer.write_u8(self.message_type as u8, data);
        buffer.write_u32(self.ack, data);
        buffer.write_u32(self.ack_bits, data);
    }

    pub fn read(data: &[u8]) -> Header {
        assert!(
            data.len() >= HEADER_SIZE,
            "data length needs to be atleast bytes {HEADER_SIZE} long."
        );

        //TODO: check data length!
        let mut buffer = IntBuffer { index: 0 };
        Header {
            seq: buffer.read_u32(data),
            //TODO: handle unwrap
            message_type: SendType::from_repr(buffer.read_u8(data)).unwrap(),
            ack: buffer.read_u32(data),
            ack_bits: buffer.read_u32(data),
        }
    }

    pub fn create_packet(header: &Header, data: Option<&[u8]>) -> Vec<u8> {
        let data_len = if let Some(d) = data { d.len() } else { 0 };

        let mut payload = vec![0_u8; data_len + HEADER_SIZE + 4];
        payload[..4].copy_from_slice(&MAGIC_NUMBER_HEADER);

        header.write(&mut payload[4..]);
        if let Some(d) = data {
            payload[HEADER_SIZE + 4..].copy_from_slice(d);
        }

        payload
    }
}
