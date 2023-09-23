use anyhow::{anyhow, bail};

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

    pub fn write(&self, data: &mut [u8], buffer: &mut IntBuffer) -> anyhow::Result<()> {
        if data.len() < HEADER_SIZE {
            bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
        }

        buffer.write_u32(self.seq, data);
        buffer.write_u8(self.packet_type as u8, data);
        buffer.write_u64(self.session_key, data);
        buffer.write_u32(self.ack, data);
        buffer.write_u32(self.ack_bits, data);

        Ok(())
    }

    pub fn read(data: &[u8]) -> anyhow::Result<Header> {
        if data.len() < HEADER_SIZE {
            bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
        }

        let mut buffer = IntBuffer { index: 0 };
        Ok(Header {
            seq: buffer.read_u32(data),
            packet_type: PacketType::from_repr(buffer.read_u8(data))
                .ok_or(anyhow!("invalid packet type"))?,
            session_key: buffer.read_u64(data),
            ack: buffer.read_u32(data),
            ack_bits: buffer.read_u32(data),
        })
    }

    pub fn create_packet(header: &Header, data: Option<&[u8]>) -> Vec<u8> {
        let mut buffer = IntBuffer { index: 0 };

        let data_len = if let Some(d) = data { d.len() } else { 0 };

        let mut payload = vec![0_u8; data_len + HEADER_SIZE + 4];
        buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
        header.write(&mut payload, &mut buffer);

        if let Some(d) = data {
            buffer.write_slice(d, &mut payload);
        }

        payload
    }
}
