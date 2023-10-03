use std::{
    collections::{HashMap, VecDeque},
    ops::{Deref, DerefMut},
};

const SIZE_STEP: usize = 128;

pub struct ArrayPool {
    pool: parking_lot::Mutex<HashMap<usize, VecDeque<Vec<u8>>>>,
}

impl ArrayPool {
    pub fn new() -> Self {
        Self {
            pool: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub fn rent(&self, size: usize) -> Vec<u8> {
        let rounded_size = ArrayPool::round_up_to_multiple_of_step(size);

        {
            let mut pool_map = self.pool.lock();

            if let Some(pool) = pool_map.get_mut(&rounded_size) {
                if let Some(data) = pool.pop_front() {
                    return data;
                }
            }
        }

        vec![0_u8; rounded_size]
    }

    pub fn free(&self, mut data: Vec<u8>) {
        //println!("z1 addr {:p}", &data);

        let reminder = data.len() % SIZE_STEP;
        if reminder != 0 {
            data.resize(ArrayPool::round_up_to_multiple_of_step(data.len()), 0);
        }

        let rounded_size = ArrayPool::round_up_to_multiple_of_step(data.len());
        let mut pool_map = self.pool.lock();

        if let Some(pool) = pool_map.get_mut(&rounded_size) {
            pool.push_back(data);
        } else {
            let mut queue = VecDeque::with_capacity(1);
            queue.push_back(data);
            pool_map.insert(rounded_size, queue);
        }
    }

    pub fn clone_pooled_vec(&mut self, data: &[u8]) -> Vec<u8> {
        let mut dest = self.rent(data.len());
        dest[..data.len()].copy_from_slice(data);
        dest
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
