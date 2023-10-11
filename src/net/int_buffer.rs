#[derive(Default)]
pub struct IntBuffer {
    pub index: usize,
}

impl IntBuffer {
    #[inline]
    pub fn new_at(index: usize) -> Self {
        Self { index }
    }

    #[inline]
    pub fn jump(&mut self, length: usize) {
        self.index += length
    }

    #[inline]
    pub fn goto(&mut self, index: usize) {
        self.index = index
    }

    #[inline]
    pub fn reset(&mut self) {
        self.index = 0;
    }

    #[inline]
    pub fn write_slice(&mut self, v: &[u8], data: &mut [u8]) {
        data[self.index..self.index + v.len()].copy_from_slice(v);
        self.index += v.len();
    }

    #[inline]
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

    #[inline]
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

    #[inline]
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

    #[inline]
    pub fn read_u32(&mut self, data: &[u8]) -> u32 {
        let value = (data[self.index] as u32)
            | (data[self.index + 1] as u32) << 8
            | (data[self.index + 2] as u32) << 16
            | (data[self.index + 3] as u32) << 24;
        self.index += 4;
        value
    }

    #[inline]
    pub fn write_u16(&mut self, v: u16, data: &mut [u8]) {
        data[self.index] = v as u8;
        self.index += 1;
        data[self.index] = (v >> 8) as u8;
        self.index += 1;
    }

    #[inline]
    pub fn read_u16(&mut self, data: &[u8]) -> u16 {
        let value = (data[self.index] as u16) | (data[self.index + 1] as u16) << 8;
        self.index += 2;
        value
    }

    #[inline]
    pub fn write_u8(&mut self, v: u8, data: &mut [u8]) {
        data[self.index] = v;
        self.index += 1;
    }

    #[inline]
    pub fn read_u8(&mut self, data: &[u8]) -> u8 {
        let value = data[self.index];
        self.index += 1;
        value
    }
}
