use std::{
    borrow::BorrowMut,
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ops::{Deref, DerefMut},
};

use log::info;
use static_init::dynamic;

const SIZE_STEP: usize = 128;

#[dynamic(drop)]
static mut POOL: HashMap<usize, VecDeque<Vec<u8>>> = HashMap::new();

pub struct ArrayPool {}

impl ArrayPool {
    pub fn rent(size: usize) -> BufferPoolRef {
        let rounded_size = ArrayPool::round_up_to_multiple_of_step(size);

        {
            let mut pool_map = POOL.write();

            if let Some(pool) = pool_map.get_mut(&rounded_size) {
                if let Some(data) = pool.pop_front() {
                    return BufferPoolRef {
                        buffer: data,
                        used: size,
                    };
                }
            }
        }

        BufferPoolRef {
            buffer: vec![0_u8; rounded_size],
            used: size,
        }
    }

    pub fn free(mut data: Vec<u8>) {
        //info!("Freeing data of length {} at addr {:p}", data.len(), &data);

        let reminder = data.len() % SIZE_STEP;
        if reminder != 0 {
            data.resize(ArrayPool::round_up_to_multiple_of_step(data.len()), 0);
        }

        let rounded_size = ArrayPool::round_up_to_multiple_of_step(data.len());
        let mut pool_map = POOL.write();

        if let Some(pool) = pool_map.get_mut(&rounded_size) {
            pool.push_back(data);
        } else {
            let mut queue = VecDeque::with_capacity(1);
            queue.push_back(data);
            pool_map.insert(rounded_size, queue);
        }
    }

    fn round_up_to_multiple_of_step(num: usize) -> usize {
        let remainder = num % SIZE_STEP;
        if remainder == 0 {
            num // Already a multiple of 128, no need to round up
        } else {
            num + (SIZE_STEP - remainder)
        }
    }
}

pub struct BufferPoolRef {
    buffer: Vec<u8>,
    pub used: usize,
}

impl BufferPoolRef {
    #[inline]
    pub fn used_data(&self) -> &[u8] {
        &self.buffer[..self.used]
    }
}

impl Deref for BufferPoolRef {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for BufferPoolRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl Drop for BufferPoolRef {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        ArrayPool::free(buffer);
    }
}
