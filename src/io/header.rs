use super::int_buffer::IntBuffer;

pub const HEADER_SIZE: usize = 12;

pub struct Header {
    pub seq: u32,
    pub ack: u32,
    pub ack_bits: u32,
}

impl Header {
    pub fn write(&self, data: &mut [u8]) {
        //TODO: check data length!
        let mut buffer = IntBuffer { index: 0 };
        buffer.write_u32(self.seq, data);
        buffer.write_u32(self.ack, data);
        buffer.write_u32(self.ack_bits, data);
    }

    pub fn read(data: &[u8]) -> Header {
        //TODO: check data length!
        let mut buffer = IntBuffer { index: 0 };
        Header {
            seq: buffer.read_u32(data),
            ack: buffer.read_u32(data),
            ack_bits: buffer.read_u32(data),
        }
    }
}
