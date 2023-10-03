use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::bail;

use crate::net::sequence::Sequence;

use super::{
    header::{Header, SendType},
    send_buffer::SendPayload,
    sequence::{SequenceBuffer, WindowSequenceBuffer},
    BUFFER_SIZE, BUFFER_WINDOW_SIZE,
};

pub const FRAGMENT_SIZE: usize = 1024;
const MAX_FRAGMENT_SIZE: usize = FRAGMENT_SIZE * u8::MAX as usize;
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

    pub fn split_fragments<'a>(&mut self, data: &'a [u8]) -> anyhow::Result<Fragments<'a>> {
        if data.len() as f32 / FRAGMENT_SIZE as f32 > u8::MAX as f32 {
            bail!("there cannot be more than 255 packets")
        }
        let chunks = data.chunks(FRAGMENT_SIZE);

        let chunk_count = chunks.len() as u8;

        let mut fragments = Fragments {
            chunk_count,
            group_id: self.group_seq,
            chunks: Vec::with_capacity(chunks.len()),
        };

        for (fragment_id, chunk) in (0_u8..u8::MAX).zip(chunks) {
            fragments.chunks.push(FragmentChunk {
                data: chunk,
                fragment_id,
            });
        }

        Sequence::increment(&mut self.group_seq);

        Ok(fragments)
    }

    pub fn insert_fragment(&mut self, header: &Header, data: &[u8]) -> anyhow::Result<bool> {
        if header.fragment_id >= header.fragment_size {
            bail!("fragment id cannot be larger than the fragment size")
        }

        if header.fragment_size == 0 {
            bail!("empty fragment with size 0")
        }

        //insert the fragment buffer if it doesnt exist yet
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
            fragment.chunks[header.fragment_id as usize] = Some(data.to_vec());
            fragment.current_size += 1;
            fragment.current_bytes += data.len();
        }

        Ok(fragment.is_done())
    }

    pub fn assemble(&mut self, group_id: u16) -> anyhow::Result<Option<Vec<u8>>> {
        let mut packet_data = None;

        if !self.validate_group(group_id) {
            self.remove_fragment_group(group_id);
        }

        if let Some(fragment) = self.fragments.get(group_id) {
            if fragment.is_done() {
                let mut parts: Vec<u8> = Vec::with_capacity(fragment.current_bytes);
                for i in 0..fragment.size {
                    let chunk: &[u8] = fragment.chunks[i as usize].as_ref().unwrap().as_ref();
                    parts.extend(chunk);
                }

                packet_data = Some(parts);
            }
        }

        //remove the fragment group
        if packet_data.is_some() {
            self.fragments.remove(group_id);
        }

        Ok(packet_data)
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
    pub chunks: Vec<Option<Vec<u8>>>,
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

pub struct Fragments<'a> {
    pub chunks: Vec<FragmentChunk<'a>>,
    pub chunk_count: u8,
    pub group_id: u16,
}

pub struct FragmentChunk<'a> {
    pub data: &'a [u8],
    pub fragment_id: u8,
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

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
        let data = vec![1, 2, 3];

        for i in 0..u8::MAX {
            let status = fragment_manager.insert_fragment(&header, &data).unwrap();
            header.fragment_id += 1;
            assert_eq!(status, i == u8::MAX - 1);
        }

        let frag_data = fragment_manager
            .assemble(header.fragment_group_id)
            .unwrap()
            .unwrap();
        assert_eq!(frag_data.len(), data.len() * u8::MAX as usize)
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
        let data = vec![1, 2, 3];

        fragment_manager.insert_fragment(&header, &data).unwrap();
        header.fragment_id += 1;

        //sleep for longer than the group timeout
        thread::sleep(GROUP_TIMEOUT + Duration::from_millis(250));

        assert!(fragment_manager.insert_fragment(&header, &data).is_err());
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
        let data = vec![1, 2, 3];

        fragment_manager.insert_fragment(&header, &data).unwrap();
        fragment_manager.insert_fragment(&header, &data).unwrap();

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
        let data = vec![1, 2, 3];

        fragment_manager.insert_fragment(&header, &data).unwrap();
        header.fragment_id += 1;

        //change the fragment size, this should throw an error
        header.fragment_size -= 1;

        assert!(fragment_manager.insert_fragment(&header, &data).is_err());
    }

    #[test]
    fn max_packet_size() {
        let mut fragment_manager: FragmentationManager = FragmentationManager::new();
        let data = [0_u8; MAX_FRAGMENT_SIZE];
        let frags_result = fragment_manager.split_fragments(&data);
        assert!(frags_result.is_ok());

        assert_eq!(frags_result.unwrap().chunk_count, u8::MAX);
    }

    #[test]
    fn too_big_packet() {
        let mut fragment_manager = FragmentationManager::new();
        let data = [0_u8; MAX_FRAGMENT_SIZE + 1];
        assert!(fragment_manager.split_fragments(&data).is_err());
    }
}
