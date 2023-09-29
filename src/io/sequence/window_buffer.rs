use super::{Sequence, SequenceBuffer};

pub struct WindowSequenceBuffer<T> {
    buffer: SequenceBuffer<T>,
    window_size: u16,
    //last sequence number to be inserted
    last_sequence: Option<u16>,
}

impl<T> WindowSequenceBuffer<T> {
    pub fn with_size(size: u16, window_size: u16) -> Self {
        assert!(
            window_size < size,
            "window size cannot be larger than the sequence buffer size."
        );

        WindowSequenceBuffer {
            buffer: SequenceBuffer::with_size(size),
            window_size,
            last_sequence: None,
        }
    }

    pub fn sequence_to_index(&self, sequence: u16) -> usize {
        self.buffer.sequence_to_index(sequence)
    }

    pub fn insert(&mut self, sequence: u16, value: T) -> Option<&mut T> {
        if let Some(last_seq) = self.last_sequence {
            if Sequence::is_greater_then(sequence, last_seq) {
                let diff = sequence.wrapping_sub(last_seq);
                let start = last_seq.wrapping_sub(self.window_size);

                let mut i = 0;
                while i < diff {
                    self.remove(i.wrapping_add(start));
                    i = i.wrapping_add(1);
                }
            }
        }

        self.last_sequence = Some(sequence);
        self.buffer.insert(sequence, value)
    }

    pub fn remove(&mut self, sequence: u16) {
        self.buffer.remove(sequence)
    }

    pub fn is_some(&self, sequence: u16) -> bool {
        self.buffer.is_some(sequence)
    }

    pub fn is_none(&self, sequence: u16) -> bool {
        self.buffer.is_none(sequence)
    }

    pub fn take(&mut self, sequence: u16) -> Option<T> {
        self.buffer.take(sequence)
    }

    pub fn get(&self, sequence: u16) -> Option<&T> {
        self.buffer.get(sequence)
    }

    pub fn get_mut(&mut self, sequence: u16) -> Option<&mut T> {
        self.buffer.get_mut(sequence)
    }
}

#[cfg(test)]
mod tests {
    const TEST_BUFFER_SIZE: u16 = 1024;
    const TEST_BUFFER_WINDOW_SIZE: u16 = 256;

    use super::*;

    #[test]
    fn insert_test() {
        let mut buffer =
            WindowSequenceBuffer::<()>::with_size(TEST_BUFFER_SIZE, TEST_BUFFER_WINDOW_SIZE);

        //directly insert into the inner buffer
        buffer.buffer.insert(4, ());
        buffer.buffer.insert(5, ());
        buffer.buffer.insert(6, ());

        buffer.insert(3 + TEST_BUFFER_WINDOW_SIZE, ());
        buffer.insert(4 + TEST_BUFFER_WINDOW_SIZE, ());

        //at this point the original values should not be removed
        assert_eq!(buffer.get(4), Some(&()));

        //at this point the sequence 4 should be gone
        buffer.insert(5 + TEST_BUFFER_WINDOW_SIZE, ());
        assert_eq!(buffer.get(4), None);

        //jump over two values and expect all original entries to be gone
        buffer.insert(7 + TEST_BUFFER_WINDOW_SIZE, ());
        assert_eq!(buffer.get(5), None);
        assert_eq!(buffer.get(6), None);
    }

    #[test]
    fn insert_with_overflow_test() {
        let mut buffer =
            WindowSequenceBuffer::<()>::with_size(TEST_BUFFER_SIZE, TEST_BUFFER_WINDOW_SIZE);

        let start = u16::MAX - 1;
        //directly insert into the inner buffer
        buffer.buffer.insert(start, ());
        buffer.buffer.insert(start.wrapping_add(1), ());
        buffer.buffer.insert(start.wrapping_add(2), ()); //0
        buffer.buffer.insert(start.wrapping_add(3), ()); //1

        //set the last_sequence value
        buffer.last_sequence = Some(start.wrapping_add(TEST_BUFFER_WINDOW_SIZE));

        //step over u16::MAX - 1 AND u16::MAX but not 0
        buffer.insert(start.wrapping_add(TEST_BUFFER_WINDOW_SIZE + 3), ());
        assert_eq!(buffer.get(u16::MAX - 1), None);
        assert_eq!(buffer.get(u16::MAX), None);
        assert_eq!(buffer.get(0), None);
        assert_eq!(buffer.get(1), Some(&()));
    }
}
