use std::sync::Arc;

use anyhow::bail;

use super::{
    bytes,
    fragmentation_manager::{FragmentationManager, FRAGMENT_SIZE},
    header::{FRAG_HEADER_SIZE, HEADER_SIZE},
    int_buffer::IntBuffer,
    Bytes, MAGIC_NUMBER_HEADER,
};

pub enum SendEvent {
    Single(Bytes),
    Fragmented(Vec<Bytes>),
}

//prepare the appropriate sized byte arrays so we don't have to reallocate and copy the data from this point on
pub fn construct_send_event(data: &[u8]) -> anyhow::Result<SendEvent> {
    let data_len = data.len();

    if data_len == 0 {
        bail!("data length cannot be 0");
    }

    if FragmentationManager::exceeds_max_length(data_len) {
        bail!("packets of this size aren't supported");
    }

    let mut int_buffer = IntBuffer::default();

    if FragmentationManager::should_fragment(data_len) {
        let chunks = data.chunks(FRAGMENT_SIZE);

        let chunk_count = chunks.len();
        let mut fragments = Vec::with_capacity(chunk_count);

        for chunk in chunks {
            int_buffer.reset();
            let buffer_size = chunk.len() + FRAG_HEADER_SIZE + 4;

            let mut buffer = bytes![buffer_size];
            int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
            int_buffer.jump(FRAG_HEADER_SIZE);
            int_buffer.write_slice(chunk, &mut buffer);

            fragments.push(buffer);
        }

        Ok(SendEvent::Fragmented(fragments))
    } else {
        let buffer_size: usize = data_len + HEADER_SIZE + 4;

        let mut buffer = bytes![buffer_size];
        int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
        int_buffer.jump(HEADER_SIZE);
        int_buffer.write_slice(data, &mut buffer);

        Ok(SendEvent::Single(buffer))
    }
}

#[cfg(test)]
mod tests {
    use bit_field::BitField;

    use crate::net::fragmentation_manager::MAX_FRAGMENT_SIZE;

    use super::*;

    #[test]
    fn send_empty_packet() {
        let data = Vec::new();
        assert!(construct_send_event(&data).is_err());
    }

    #[test]
    fn packet_exceeds_max_size() {
        let buffer = bytes![MAX_FRAGMENT_SIZE + 1];
        assert!(construct_send_event(&buffer).is_err());
    }

    #[test]
    fn packet_max_size() {
        let buffer = bytes![MAX_FRAGMENT_SIZE];

        assert!(construct_send_event(&buffer).is_ok());
    }

    #[test]
    fn test_single_packet() {
        let buffer = bytes![FRAGMENT_SIZE];

        assert!(matches!(
            construct_send_event(&buffer).unwrap(),
            SendEvent::Single(buffer)
        ));
    }

    #[test]
    fn test_fragmented_packet() {
        let buffer = bytes![FRAGMENT_SIZE + 1];

        assert!(matches!(
            construct_send_event(&buffer).unwrap(),
            SendEvent::Fragmented(buffer)
        ));
    }
}
