use std::sync::Arc;

use anyhow::{anyhow, bail};

use crate::net::PacketType;

use super::{int_buffer::IntBuffer, MAGIC_NUMBER_HEADER};

pub const HEADER_SIZE: usize = 17;
pub const FRAG_HEADER_SIZE: usize = 21;

#[derive(PartialEq, Eq)]
pub enum SendType {
    Reliable,
    Unreliable,
}

#[derive(Debug, Clone, Copy)]
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

    pub fn write(&self, data: &mut [u8], int_buffer: &mut IntBuffer) -> anyhow::Result<()> {
        if data.len() - int_buffer.index < HEADER_SIZE {
            bail!("data length needs to be at least bytes {HEADER_SIZE} long.");
        }

        int_buffer.write_u16(self.seq, data);
        int_buffer.write_u8(self.packet_type as u8, data);
        int_buffer.write_u64(self.session_key, data);
        int_buffer.write_u16(self.ack, data);
        int_buffer.write_u32(self.ack_bits, data);

        if self.packet_type.is_frag_variant() {
            if data.len() - int_buffer.index < 4 {
                bail!("data length needs to be at least bytes {FRAG_HEADER_SIZE} long.");
            }

            int_buffer.write_u16(self.fragment_group_id, data);
            int_buffer.write_u8(self.fragment_id, data);
            int_buffer.write_u8(self.fragment_size, data);
        }

        Ok(())
    }

    pub fn read(data: &[u8]) -> anyhow::Result<Header> {
        if data.len() < HEADER_SIZE {
            bail!("data length needs to be at least bytes {HEADER_SIZE} long.");
        }

        let mut int_buffer = IntBuffer::default();

        let seq = int_buffer.read_u16(data);
        let packet_type = PacketType::try_from(int_buffer.read_u8(data))?;
        let session_key = int_buffer.read_u64(data);
        let ack = int_buffer.read_u16(data);
        let ack_bits = int_buffer.read_u32(data);

        let mut fragment_group_id = 0;
        let mut fragment_id = 0;
        let mut fragment_size = 0;

        if packet_type.is_frag_variant() {
            if data.len() - int_buffer.index < 4 {
                bail!("data length needs to be at least bytes {FRAG_HEADER_SIZE} long.");
            }

            fragment_group_id = int_buffer.read_u16(data);
            fragment_id = int_buffer.read_u8(data);
            fragment_size = int_buffer.read_u8(data);
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

    pub fn get_header_size(&self) -> usize {
        if self.packet_type.is_frag_variant() {
            FRAG_HEADER_SIZE
        } else {
            HEADER_SIZE
        }
    }

    #[inline]
    pub const fn max_header_size() -> usize {
        FRAG_HEADER_SIZE
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Deref, thread};

    use crate::net::bytes;

    use super::*;

    #[test]
    fn header_write_insufficient_size() {
        let header = Header::new(0, 0, SendType::Reliable, false);
        let mut buffer = vec![0_u8; header.get_header_size() - 1];
        assert!(header
            .write(&mut buffer, &mut IntBuffer::default())
            .is_err());
    }

    #[test]
    fn fragment_header_write_insufficient_size() {
        let header = Header::new(0, 0, SendType::Reliable, true);
        let mut buffer = vec![0_u8; header.get_header_size() - 1];
        assert!(header
            .write(&mut buffer, &mut IntBuffer::default())
            .is_err());
    }

    #[test]
    fn header_write() {
        let mut header = Header::new(1, 2, SendType::Reliable, false);
        header.ack = 3;
        header.ack_bits = 4;

        //offset it by 5 to test if the bound checks work
        let mut int_buffer = IntBuffer::new_at(5);

        let mut buffer = vec![0_u8; header.get_header_size() + 5];
        assert!(header.write(&mut buffer, &mut IntBuffer::new_at(5)).is_ok());

        int_buffer.goto(5);
        assert_eq!(int_buffer.read_u16(&buffer), 1);
        assert_eq!(
            int_buffer.read_u8(&buffer),
            PacketType::PayloadReliable as u8
        );
        assert_eq!(int_buffer.read_u64(&buffer), 2);
        assert_eq!(int_buffer.read_u16(&buffer), 3);
        assert_eq!(int_buffer.read_u32(&buffer), 4);
    }

    #[test]
    fn fragment_header_write() {
        let mut header = Header::new(1, 2, SendType::Reliable, true);
        header.ack = 3;
        header.ack_bits = 4;
        header.fragment_group_id = 5;
        header.fragment_id = 6;
        header.fragment_size = 7;

        //offset it by 5 to test if the bound checks work
        let mut int_buffer = IntBuffer::new_at(5);

        let mut buffer = vec![0_u8; header.get_header_size() + 5];
        assert!(header.write(&mut buffer, &mut IntBuffer::new_at(5)).is_ok());

        int_buffer.goto(5);
        assert_eq!(int_buffer.read_u16(&buffer), 1);
        assert_eq!(
            int_buffer.read_u8(&buffer),
            PacketType::PayloadReliableFrag as u8
        );
        assert_eq!(int_buffer.read_u64(&buffer), 2);
        assert_eq!(int_buffer.read_u16(&buffer), 3);
        assert_eq!(int_buffer.read_u32(&buffer), 4);
        assert_eq!(int_buffer.read_u16(&buffer), 5);
        assert_eq!(int_buffer.read_u8(&buffer), 6);
        assert_eq!(int_buffer.read_u8(&buffer), 7);
    }

    #[test]
    fn header_read_insufficient_size() {
        let mut buffer = vec![0_u8; HEADER_SIZE - 1];
        assert!(Header::read(&buffer).is_err());
    }

    #[test]
    fn header_read() {
        let mut header = Header::new(1, 2, SendType::Reliable, false);
        header.ack = 3;
        header.ack_bits = 4;

        let mut buffer = vec![0_u8; header.get_header_size()];
        assert!(header.write(&mut buffer, &mut IntBuffer::default()).is_ok());

        let new_header = Header::read(&buffer);
        assert!(new_header.is_ok());
        let new_header = new_header.unwrap();

        assert_eq!(header.seq, new_header.seq);
        assert_eq!(header.packet_type, new_header.packet_type);
        assert_eq!(header.session_key, new_header.session_key);
        assert_eq!(header.ack, new_header.ack);
        assert_eq!(header.ack_bits, new_header.ack_bits);
    }

    #[test]
    fn fragmented_header_read() {
        let mut header = Header::new(1, 2, SendType::Reliable, true);
        header.ack = 3;
        header.ack_bits = 4;
        header.fragment_group_id = 5;
        header.fragment_id = 6;
        header.fragment_size = 7;

        let mut buffer = vec![0_u8; header.get_header_size()];
        assert!(header.write(&mut buffer, &mut IntBuffer::default()).is_ok());

        let new_header = Header::read(&buffer);
        assert!(new_header.is_ok());
        let new_header = new_header.unwrap();

        assert_eq!(header.seq, new_header.seq);
        assert_eq!(header.packet_type, new_header.packet_type);
        assert_eq!(header.session_key, new_header.session_key);
        assert_eq!(header.ack, new_header.ack);
        assert_eq!(header.ack_bits, new_header.ack_bits);
        assert_eq!(header.fragment_group_id, new_header.fragment_group_id);
        assert_eq!(header.fragment_id, new_header.fragment_id);
        assert_eq!(header.fragment_size, new_header.fragment_size);
    }
}
