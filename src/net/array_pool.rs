/*use std::{
    borrow::BorrowMut,
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ops::{Deref, DerefMut},
};

use log::info;
use static_init::dynamic;

const SIZE_STEP: usize = 128;

#[dynamic(drop)]
static mut POOL: HashMap<usize, Vec<Bytes>> = HashMap::new();*/

/*
A potential way of not using static pools could be if we create two structs:
 - ArrayPool which contains an vec of byte arrays wrapped in a mutex
 - ArrayPoolManager - which has an Arc<ArrayPool> inside it

 The ArrayPoolManager actually generates new BufferPoolRef objects and clones the Arc<ArrayPool> with it
 And on BufferPoolRef drop we can free it directly back into ArrayPool
*/

//WARN: this isn't even any faster than the default allocator. Get rid of it. it gets useful at 4KB+ allocations

/*pub struct ArrayPool {}

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

    pub fn copy_from_slice(data: &[u8]) -> BufferPoolRef {
        let mut buffer = ArrayPool::rent(data.len());
        buffer.copy_from_slice(data);
        buffer
    }

    pub fn free(mut data: Bytes) {
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
    buffer: Bytes,
    used: usize,
}

impl BufferPoolRef {
    #[inline]
    pub fn len(&self) -> usize {
        self.used
    }

    #[inline]
    pub fn left_shift(&mut self, length: usize) {
        self.buffer[..self.used].rotate_left(length);
        self.used -= length;
    }
}

impl Deref for BufferPoolRef {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer[..self.used]
    }
}

impl DerefMut for BufferPoolRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer[..self.used]
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

    #[test]
    fn buffer_deref() {
        let buffer = ArrayPool::rent(50);
        let data: &[u8] = &buffer;
        assert_eq!(data.len(), 50);
    }

    #[test]
    fn buffer_mut_deref() {
        let buffer = ArrayPool::rent(50);
        let mut data: &[u8] = &buffer;
        assert_eq!(data.len(), 50);
    }

    #[test]
    fn buffer_left_shift() {
        let mut buffer = ArrayPool::rent(5);
        let data = &[1, 2, 3, 4, 5];

        buffer.copy_from_slice(data);

        buffer.left_shift(3);
        assert_eq!(buffer.len(), 2);

        assert_eq!(buffer.deref(), &data[3..5]);
    }

    #[test]
    fn speed_test() {
        for i in 0..100000 {
            ArrayPool::free(vec![0_u8; SIZE_STEP]);
        }
        let start = Instant::now();
        let mut v = Vec::with_capacity(100000);
        for i in 0..100000 {
            v.push(ArrayPool::rent(SIZE_STEP * 50));
        }
        println!("elapsed1 {}", start.elapsed().as_millis());
        assert!(start.elapsed() < Duration::from_millis(1000));

        let start1 = Instant::now();
        {
            let mut v2 = Vec::with_capacity(100000);
            for i in 0..100000 {
                v2.push(vec![0_u8; SIZE_STEP * 50]);
            }
        }

        println!("elapsed2 {}", start1.elapsed().as_millis());
        assert!(start1.elapsed() < Duration::from_millis(1000));
    }
}*/
