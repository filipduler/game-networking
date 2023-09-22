use crate::io::PacketType;

use super::{int_buffer::IntBuffer, MAGIC_NUMBER_HEADER};

pub const HEADER_SIZE: usize = 21;

pub enum SendType {
    Reliable,
    Unreliable,
}

pub struct Header {
    pub seq: u32,
    pub packet_type: PacketType,
    pub session_key: u64,
    pub ack: u32,
    pub ack_bits: u32,
}

impl Header {
    pub fn new(seq: u32, session_key: u64, send_type: SendType) -> Self {
        Self {
            seq,
            session_key,
            packet_type: match send_type {
                SendType::Reliable => PacketType::PayloadReliable,
                SendType::Unreliable => PacketType::PayloadUnreliable,
            },
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
        buffer.write_u8(self.packet_type as u8, data);
        buffer.write_u64(self.session_key, data);
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
            session_key: buffer.read_u64(data),
            packet_type: PacketType::from_repr(buffer.read_u8(data)).unwrap(),
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
