use std::sync::Arc;

use anyhow::{anyhow, bail};

use crate::net::PacketType;

use super::{array_pool::ArrayPool, int_buffer::IntBuffer, MAGIC_NUMBER_HEADER};

pub const HEADER_SIZE: usize = 17;
pub const FRAG_HEADER_SIZE: usize = 21;

#[derive(PartialEq, Eq)]
pub enum SendType {
    Reliable,
    Unreliable,
}

pub struct Header {
    pub seq: u16,
    pub packet_type: PacketType,
    pub session_key: u64,
    pub ack: u16,
    pub ack_bits: u32,

    //optional fragment part
    pub fragment_group_id: u16,
    pub fragment_id: u8,
    pub fragment_size: u8,
}

impl Header {
    pub fn new(seq: u16, session_key: u64, send_type: SendType, frag: bool) -> Self {
        Self {
            seq,
            session_key,
            packet_type: match send_type {
                SendType::Reliable => {
                    if frag {
                        PacketType::PayloadReliableFrag
                    } else {
                        PacketType::PayloadReliable
                    }
                }
                SendType::Unreliable => {
                    if frag {
                        PacketType::PayloadUnreliableFrag
                    } else {
                        PacketType::PayloadUnreliable
                    }
                }
            },
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: 0,
        }
    }

    pub fn write(&self, data: &mut [u8], buffer: &mut IntBuffer) -> anyhow::Result<()> {
        if data.len() < HEADER_SIZE {
            bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
        }

        buffer.write_u16(self.seq, data);
        buffer.write_u8(self.packet_type as u8, data);
        buffer.write_u64(self.session_key, data);
        buffer.write_u16(self.ack, data);
        buffer.write_u32(self.ack_bits, data);

        if self.packet_type.is_frag_variant() {
            if data.len() < FRAG_HEADER_SIZE {
                bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
            }

            buffer.write_u16(self.fragment_group_id, data);
            buffer.write_u8(self.fragment_id, data);
            buffer.write_u8(self.fragment_size, data);
        }

        Ok(())
    }

    pub fn read(data: &[u8]) -> anyhow::Result<Header> {
        if data.len() < HEADER_SIZE {
            bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
        }

        let mut buffer = IntBuffer { index: 0 };

        let seq = buffer.read_u16(data);
        let packet_type =
            PacketType::from_repr(buffer.read_u8(data)).ok_or(anyhow!("invalid packet type"))?;
        let session_key = buffer.read_u64(data);
        let ack = buffer.read_u16(data);
        let ack_bits = buffer.read_u32(data);

        let mut fragment_group_id = 0;
        let mut fragment_id = 0;
        let mut fragment_size = 0;

        if packet_type.is_frag_variant() {
            if data.len() < FRAG_HEADER_SIZE {
                bail!("data length needs to be atleast bytes {HEADER_SIZE} long.");
            }

            fragment_group_id = buffer.read_u16(data);
            fragment_id = buffer.read_u8(data);
            fragment_size = buffer.read_u8(data);

            //fragment id is 0 indexed
            if fragment_id >= fragment_size {
                bail!("fragment id cannot be larger or equal to fragment size");
            }
        }

        Ok(Header {
            seq,
            packet_type,
            session_key,
            ack,
            ack_bits,
            fragment_group_id,
            fragment_id,
            fragment_size,
        })
    }

    pub fn create_packet(
        &self,
        data: Option<&[u8]>,
        array_pool: &Arc<ArrayPool>,
    ) -> (Vec<u8>, usize) {
        let mut buffer = IntBuffer::new_at(0);

        let header_size = self.get_header_size();

        let data_len = if let Some(d) = data { d.len() } else { 0 };

        let packet_length = data_len + header_size + 4;
        let mut payload = array_pool.rent(packet_length);
        buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
        self.write(&mut payload, &mut buffer);

        if let Some(d) = data {
            buffer.write_slice(d, &mut payload);
        }

        (payload, packet_length)
    }

    pub fn get_header_size(&self) -> usize {
        if self.packet_type.is_frag_variant() {
            FRAG_HEADER_SIZE
        } else {
            HEADER_SIZE
        }
    }

    #[inline]
    pub fn max_header_size() -> usize {
        FRAG_HEADER_SIZE
    }
}
