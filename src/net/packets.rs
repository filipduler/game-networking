use std::sync::Arc;

use anyhow::bail;

use super::{
    array_pool::{ArrayPool, BufferPoolRef},
    fragmentation_manager::{FragmentationManager, FRAGMENT_SIZE},
    header::{FRAG_HEADER_SIZE, HEADER_SIZE},
};

pub enum SendEvent {
    Single(BufferPoolRef),
    Fragmented(Vec<(BufferPoolRef)>),
}

//prepare the appropriate sized byte arrays so we dont have to reallocate and copy the data from this point on
pub fn construct_send_event(data: &[u8]) -> anyhow::Result<SendEvent> {
    let data_len = data.len();

    if data_len == 0 {
        bail!("data length cannot be 0");
    }

    if FragmentationManager::exceeds_max_length(data_len) {
        bail!("packets of this size arent supported");
    }

    if FragmentationManager::should_fragment(data_len) {
        let chunks = data.chunks(FRAGMENT_SIZE);

        let chunk_count = chunks.len();
        let mut fragments = Vec::with_capacity(chunk_count);

        for chunk in chunks {
            let buffer_size = chunk.len() + FRAG_HEADER_SIZE + 4;
            let mut buffer = ArrayPool::rent(buffer_size);
            buffer.copy_slice(chunk);
            fragments.push(buffer);
        }

        Ok(SendEvent::Fragmented(fragments))
    } else {
        let buffer_size: usize = data_len + HEADER_SIZE + 4;
        let mut buffer = ArrayPool::rent(buffer_size);
        buffer.copy_slice(data);

        Ok(SendEvent::Single(buffer))
    }
}

