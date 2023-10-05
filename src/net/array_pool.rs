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
static mut POOL: HashMap<usize, Vec<Vec<u8>>> = HashMap::new();

pub struct ArrayPool {}

impl ArrayPool {
    pub fn rent(size: usize) -> BufferPoolRef {
        let rounded_size = ArrayPool::round_up_to_multiple_of_step(size);

        {
            let mut pool_map = POOL.write();

            if let Some(pool) = pool_map.get_mut(&rounded_size) {
                if let Some(data) = pool.pop() {
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

        assert!(
            data.len() % SIZE_STEP == 0,
            "data length has to be a multiple of {SIZE_STEP}"
        );

        let rounded_size = ArrayPool::round_up_to_multiple_of_step(data.len());
        let mut pool_map = POOL.write();

        if let Some(pool) = pool_map.get_mut(&rounded_size) {
            pool.push(data);
        } else {
            let queue = vec![data];
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

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use crate::net::array_pool::ArrayPool;

    use super::*;

    #[test]
    fn rent_and_free() {
        {
            let buffer = ArrayPool::rent(SIZE_STEP);
        }
        assert_eq!(POOL.read().get(&SIZE_STEP).unwrap().len(), 1);
    }

    #[test]
    fn rounding_requested_size_to_multiple() {
        let buffer = ArrayPool::rent(SIZE_STEP - 1);
        assert_eq!(buffer.buffer.len(), SIZE_STEP);
    }

    /*#[test]
    fn speed_test() {
        for i in 0..100000 {
            ArrayPool::free(vec![0_u8; SIZE_STEP]);
        }
        let mut v = Vec::with_capacity(100000);
        let start = Instant::now();
        for i in 0..100000 {
            v.push(ArrayPool::rent(SIZE_STEP));
        }
        println!("elapsed {}", start.elapsed().as_millis());
        assert!(start.elapsed() < Duration::from_millis(50));
    }*/
}
