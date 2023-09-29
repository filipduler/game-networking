use std::rc::Rc;

use anyhow::bail;

use super::{
    header::{Header, SendType},
    send_buffer::SendPayload,
    sequence::SequenceBuffer,
    BUFFER_SIZE, FRAGMENT_SIZE,
};

pub struct FragmentationManager {
    fragment_group_seq: u16,
    fragment_buffer: SequenceBuffer<ReceiveFragments>,
}

impl FragmentationManager {
    pub fn new() -> Self {
        Self {
            fragment_group_seq: 0,
            fragment_buffer: SequenceBuffer::with_size(BUFFER_SIZE),
        }
    }

    pub fn should_fragment(length: usize) -> bool {
        length > FRAGMENT_SIZE
    }

    pub fn split_fragments<'a>(&mut self, data: &'a [u8]) -> Fragments<'a> {
        let chunks = data.chunks(FRAGMENT_SIZE);
        assert!(
            chunks.len() <= u8::MAX as usize,
            "there cannot be more than 255 packets"
        );

        let mut fragment_id: u8 = 0;
        let chunk_count = chunks.len() as u8;

        let mut fragments = Fragments {
            chunk_count,
            group_id: self.fragment_group_seq,
            chunks: Vec::with_capacity(chunks.len()),
        };

        for chunk in chunks {
            fragments.chunks.push(FragmentChunk {
                data: chunk,
                fragment_id,
            });
            fragment_id += 1;
        }
        self.fragment_group_seq += 1;

        fragments
    }

    pub fn insert_fragment(&mut self, header: &Header, data: &[u8]) -> anyhow::Result<bool> {
        //insert the fragment buffer if it doesnt exist yet
        if self.fragment_buffer.is_none(header.fragment_group_id) {
            self.fragment_buffer.insert(
                header.fragment_group_id,
                ReceiveFragments {
                    group_id: header.fragment_group_id,
                    chunks: (0..header.fragment_size).map(|_| None).collect(),
                    size: header.fragment_size,
                    current_size: 0,
                    current_bytes: 0,
                },
            );
        }

        //we can safely expect because we inserted the entry above
        let mut fragment = self
            .fragment_buffer
            .get_mut(header.fragment_group_id)
            .expect("fragment not set in the buffer");

        if header.fragment_id >= fragment.size {
            bail!("fragment id cannot be larger than the fragment size")
        }

        if fragment.chunks[header.fragment_id as usize].is_none() {
            fragment.chunks[header.fragment_id as usize] = Some(data.to_vec());
            fragment.current_size += 1;
            fragment.current_bytes += data.len();
        }

        Ok(fragment.is_done())
    }

    pub fn build_fragment(&self, group_id: u16) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(fragment) = self.fragment_buffer.get(group_id) {
            if fragment.is_done() {
                let mut parts: Vec<u8> = Vec::with_capacity(fragment.current_bytes);
                for i in 0..fragment.size {
                    let chunk: &[u8] = fragment.chunks[i as usize].as_ref().unwrap().as_ref();
                    parts.extend(chunk);
                }

                return Ok(Some(parts));
            }
        }

        Ok(None)
    }
}

pub struct ReceiveFragments {
    pub group_id: u16,
    pub chunks: Vec<Option<Vec<u8>>>,
    pub size: u8,
    pub current_size: u8,
    pub current_bytes: usize,
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
