use std::{
    collections::VecDeque,
    rc::Rc,
    slice::Chunks,
    time::{Duration, Instant},
};

use anyhow::bail;

use crate::net::sequence::Sequence;

use super::{
    header::{Header, SendType},
    send_buffer::SendPayload,
    sequence::{SequenceBuffer, WindowSequenceBuffer},
    Bytes, BUFFER_SIZE, BUFFER_WINDOW_SIZE,
};

pub const FRAGMENT_SIZE: usize = 1024;
pub const MAX_FRAGMENT_SIZE: usize = FRAGMENT_SIZE * u8::MAX as usize;
const GROUP_TIMEOUT: Duration = Duration::from_secs(5);

pub struct FragmentationManager {
    group_seq: u16,
    fragments: WindowSequenceBuffer<ReceiveFragments>,
}

impl FragmentationManager {
    pub fn new() -> Self {
        Self {
            group_seq: 0,
            fragments: WindowSequenceBuffer::with_size(BUFFER_SIZE, BUFFER_WINDOW_SIZE),
        }
    }

    pub fn should_fragment(length: usize) -> bool {
        length > FRAGMENT_SIZE
    }

    pub fn split_fragments(&mut self, chunks: Vec<Bytes>) -> anyhow::Result<Fragments> {
        if chunks.len() > u8::MAX as usize {
            bail!("cannot create a fragmented message from more than 255 chunks");
        }

        let chunk_count = chunks.len() as u8;

        let mut fragments = Fragments {
            chunk_count,
            group_id: self.group_seq,
            chunks: Vec::with_capacity(chunks.len()),
        };

        for (fragment_id, chunk) in (0_u8..u8::MAX).zip(chunks) {
            fragments.chunks.push(FragmentChunk {
                buffer: chunk,
                fragment_id,
            });
        }

        Sequence::increment(&mut self.group_seq);

        Ok(fragments)
    }

    pub fn insert_fragment(&mut self, header: &Header, buffer: Bytes) -> anyhow::Result<bool> {
        if header.fragment_size == 0 {
            bail!("empty fragment with size 0")
        }

        if header.fragment_id >= header.fragment_size {
            bail!("fragment id cannot be larger than the fragment size")
        }

        //insert the fragment buffer if it doesn't exist yet
        if self.fragments.is_none(header.fragment_group_id) {
            self.fragments.insert(
                header.fragment_group_id,
                ReceiveFragments {
                    group_id: header.fragment_group_id,
                    chunks: (0..header.fragment_size).map(|_| None).collect(),
                    size: header.fragment_size,
                    current_size: 0,
                    current_bytes: 0,
                    created_on: Instant::now(),
                },
            );
        }

        if !self.validate_group(header.fragment_group_id) {
            self.remove_fragment_group(header.fragment_group_id);
            bail!("fragment has timed out")
        }

        //we can safely expect because we inserted the entry above
        let mut fragment = self
            .fragments
            .get_mut(header.fragment_group_id)
            .expect("fragment not set in the buffer");

        if header.fragment_size != fragment.size {
            bail!("fragment sizes do not match")
        }

        if fragment.chunks[header.fragment_id as usize].is_none() {
            let buffer_len = buffer.len();

            fragment.chunks[header.fragment_id as usize] = Some(buffer);
            fragment.current_size += 1;
            fragment.current_bytes += buffer_len;
        }

        Ok(fragment.is_done())
    }

    pub fn assemble(&mut self, group_id: u16) -> anyhow::Result<Vec<Bytes>> {
        if !self.validate_group(group_id) {
            self.remove_fragment_group(group_id);
            bail!("fragment group has expired");
        }

        if let Some(fragment) = self.fragments.get(group_id) {
            if !fragment.is_done() {
                bail!("fragment is not done yet");
            }
        } else {
            bail!("fragment not found");
        }

        let mut fragment = self.fragments.take(group_id).unwrap();

        let mut parts = Vec::with_capacity(fragment.current_size as usize);
        for i in 0..fragment.size {
            if let Some(chunk) = fragment.chunks.pop_front().unwrap() {
                parts.push(chunk);
            } else {
                bail!("missing chunk on index {i}");
            }
        }

        Ok(parts)
    }

    fn validate_group(&self, group_id: u16) -> bool {
        if let Some(fragment) = self.fragments.get(group_id) {
            return fragment.created_on.elapsed() < GROUP_TIMEOUT;
        }
        false
    }

    fn remove_fragment_group(&mut self, group_id: u16) {
        self.fragments.remove(group_id);
    }

    pub fn exceeds_max_length(length: usize) -> bool {
        MAX_FRAGMENT_SIZE < length
    }
}

pub struct ReceiveFragments {
    pub group_id: u16,
    pub chunks: VecDeque<Option<Bytes>>,
    pub size: u8,
    pub current_size: u8,
    pub current_bytes: usize,
    pub created_on: Instant,
}

impl ReceiveFragments {
    fn is_done(&self) -> bool {
        self.current_size == self.size
    }
}

pub struct Fragments {
    pub chunks: Vec<FragmentChunk>,
    pub chunk_count: u8,
    pub group_id: u16,
}

pub struct FragmentChunk {
    pub buffer: Bytes,
    pub fragment_id: u8,
}

#[cfg(test)]
mod tests {
    use std::{ops::Deref, thread};

    use crate::net::bytes;

    use super::*;

    #[test]
    fn valid_chunk_sequence() {
        let mut fragment_manager = FragmentationManager::new();
        let mut header = Header {
            seq: 0,
            packet_type: crate::net::PacketType::PayloadReliable,
            session_key: 0,
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: 5,
        };

        let mut seq = 0;
        for i in 0..5 {
            let mut data = bytes!(3);
            data[0] = i;
            data[1] = i;
            data[2] = i;

            let status = fragment_manager.insert_fragment(&header, data).unwrap();
            header.fragment_id += 1;
        }

        let frag_data = fragment_manager.assemble(header.fragment_group_id).unwrap();

        for i in 0..5_u8 {
            assert_eq!(frag_data[i as usize].deref(), &[i, i, i]);
        }
    }

    #[test]
    fn full_fragment_insert_and_build() {
        let mut fragment_manager = FragmentationManager::new();
        let mut header = Header {
            seq: 0,
            packet_type: crate::net::PacketType::PayloadReliable,
            session_key: 0,
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: u8::MAX,
        };

        for i in 0..u8::MAX {
            let data = bytes!(3);

            let status = fragment_manager.insert_fragment(&header, data).unwrap();
            header.fragment_id += 1;
            assert_eq!(status, i == u8::MAX - 1);
        }

        let frag_data = fragment_manager.assemble(header.fragment_group_id).unwrap();

        assert_eq!(
            frag_data.into_iter().map(|x| x.len()).sum::<usize>(),
            3 * u8::MAX as usize
        )
    }

    #[test]
    fn fragment_group_timeout() {
        let mut fragment_manager = FragmentationManager::new();
        let mut header = Header {
            seq: 0,
            packet_type: crate::net::PacketType::PayloadReliable,
            session_key: 0,
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: 2,
        };

        fragment_manager
            .insert_fragment(&header, bytes!(3))
            .unwrap();
        header.fragment_id += 1;

        //sleep for longer than the group timeout
        thread::sleep(GROUP_TIMEOUT + Duration::from_millis(250));

        assert!(fragment_manager
            .insert_fragment(&header, bytes!(3))
            .is_err());
        //check the fragment group was removed
        assert!(fragment_manager.fragments.is_none(header.fragment_group_id));
    }

    #[test]
    fn insert_duplicate_packet() {
        let mut fragment_manager = FragmentationManager::new();
        let mut header = Header {
            seq: 0,
            packet_type: crate::net::PacketType::PayloadReliable,
            session_key: 0,
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: u8::MAX,
        };

        fragment_manager
            .insert_fragment(&header, bytes!(3))
            .unwrap();
        fragment_manager
            .insert_fragment(&header, bytes!(3))
            .unwrap();

        let frag = fragment_manager
            .fragments
            .get(header.fragment_group_id)
            .unwrap();
        assert_eq!(frag.current_bytes, 3);
        assert_eq!(frag.current_size, 1);
    }

    #[test]
    fn insert_different_fragment_sizes() {
        let mut fragment_manager = FragmentationManager::new();
        let mut header = Header {
            seq: 0,
            packet_type: crate::net::PacketType::PayloadReliable,
            session_key: 0,
            ack: 0,
            ack_bits: 0,
            fragment_group_id: 0,
            fragment_id: 0,
            fragment_size: u8::MAX,
        };

        fragment_manager
            .insert_fragment(&header, bytes!(3))
            .unwrap();
        header.fragment_id += 1;

        //change the fragment size, this should throw an error
        header.fragment_size -= 1;

        assert!(fragment_manager
            .insert_fragment(&header, bytes!(3))
            .is_err());
    }

    #[test]
    fn max_packet_size() {
        let mut fragment_manager: FragmentationManager = FragmentationManager::new();

        let mut frags = Vec::with_capacity(u8::MAX as usize);
        for chunk in 0..u8::MAX {
            frags.push(bytes!(FRAGMENT_SIZE));
        }

        let frags_result = fragment_manager.split_fragments(frags);
        assert!(frags_result.is_ok());

        assert_eq!(frags_result.unwrap().chunk_count, u8::MAX);
    }

    #[test]
    fn packet_too_large() {
        let mut fragment_manager = FragmentationManager::new();
        let mut frags = Vec::with_capacity(u8::MAX as usize + 1);
        for chunk in 0..u8::MAX as usize + 1 {
            frags.push(bytes!(FRAGMENT_SIZE));
        }

        assert!(fragment_manager.split_fragments(frags).is_err());
    }
}
