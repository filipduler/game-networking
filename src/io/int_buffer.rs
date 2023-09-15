pub struct IntBuffer {
    pub index: usize,
}

impl IntBuffer {
    pub fn write_slice(&mut self, v: &[u8], data: &mut [u8]) {
        data[..v.len()].copy_from_slice(v);
    }

    pub fn write_u64(&mut self, v: u64, data: &mut [u8]) {
        data[self.index] = v as u8;
        self.index += 1;
        data[self.index] = (v >> 8) as u8;
        self.index += 1;
        data[self.index] = (v >> 16) as u8;
        self.index += 1;
        data[self.index] = (v >> 24) as u8;
        self.index += 1;

        data[self.index] = (v >> 32) as u8;
        self.index += 1;
        data[self.index] = (v >> 40) as u8;
        self.index += 1;
        data[self.index] = (v >> 48) as u8;
        self.index += 1;
        data[self.index] = (v >> 56) as u8;
        self.index += 1;
    }

    pub fn read_u64(&mut self, data: &[u8]) -> u64 {
        let value = (data[self.index] as u64)
            | (data[self.index + 1] as u64) << 8
            | (data[self.index + 2] as u64) << 16
            | (data[self.index + 3] as u64) << 24
            | (data[self.index + 4] as u64) << 32
            | (data[self.index + 5] as u64) << 40
            | (data[self.index + 6] as u64) << 48
            | (data[self.index + 7] as u64) << 56;
        self.index += 8;
        value
    }

    pub fn write_u32(&mut self, v: u32, data: &mut [u8]) {
        data[self.index] = v as u8;
        self.index += 1;
        data[self.index] = (v >> 8) as u8;
        self.index += 1;
        data[self.index] = (v >> 16) as u8;
        self.index += 1;
        data[self.index] = (v >> 24) as u8;
        self.index += 1;
    }

    pub fn read_u32(&mut self, data: &[u8]) -> u32 {
        let value = (data[self.index] as u32)
            | (data[self.index + 1] as u32) << 8
            | (data[self.index + 2] as u32) << 16
            | (data[self.index + 3] as u32) << 24;
        self.index += 4;
        value
    }

    pub fn write_u16(&mut self, v: u16, data: &mut [u8]) {
        data[self.index] = v as u8;
        self.index += 1;
        data[self.index] = (v >> 8) as u8;
        self.index += 1;
    }

    pub fn read_u16(&mut self, data: &[u8]) -> u16 {
        let value = (data[self.index] as u16) | (data[self.index + 1] as u16) << 8;
        self.index += 2;
        value
    }

    pub fn write_u8(&mut self, v: u8, data: &mut [u8]) {
        data[self.index] = v;
        self.index += 1;
    }

    pub fn read_u8(&mut self, data: &[u8]) -> u8 {
        let value = data[self.index];
        self.index += 1;
        value
    }

    pub fn u4_to_u8(v1: u8, v2: u8) -> u8 {
        v1 | v2 << 4
    }

    pub fn u8_to_u4(byte: u8) -> (u8, u8) {
        (byte & 0x0F, byte >> 4)
    }
}
