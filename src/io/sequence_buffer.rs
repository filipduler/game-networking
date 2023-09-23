#[derive(Clone)]
pub struct SequenceBuffer<T> {
    pub values: Vec<Option<T>>,
    pub partition_by: u32,
}

impl<T> SequenceBuffer<T> {
    pub fn with_capacity(size: u32) -> Self {
        let mut values = Vec::with_capacity(size as usize);
        for _ in 0..size {
            values.push(None);
        }

        SequenceBuffer {
            values,
            partition_by: size,
        }
    }

    pub fn sequence_to_index(&self, sequence: u32) -> usize {
        (sequence % self.partition_by) as usize
    }

    pub fn insert(&mut self, sequence: u32, value: T) -> Option<&mut T> {
        let index = self.sequence_to_index(sequence);
        self.values[index] = Some(value);
        return self.values[index].as_mut();
    }

    pub fn remove(&mut self, sequence: u32) {
        let index = self.sequence_to_index(sequence);
        self.values[index] = None;
    }

    pub fn remove_at_index(&mut self, index: usize) {
        self.values[index] = None;
    }

    pub fn is_some(&self, sequence: u32) -> bool {
        let index = self.sequence_to_index(sequence);
        self.values[index].is_some()
    }

    pub fn is_none(&self, sequence: u32) -> bool {
        let index = self.sequence_to_index(sequence);
        self.values[index].is_none()
    }

    pub fn take(&mut self, sequence: u32) -> Option<T> {
        let index = self.sequence_to_index(sequence);
        self.values[index].take()
    }

    pub fn get(&self, sequence: u32) -> Option<&T> {
        let index = self.sequence_to_index(sequence);
        match self.values.get(index) {
            Some(value) => value.as_ref(),
            None => None,
        }
    }

    pub fn get_at_index(&self, index: usize) -> Option<&T> {
        match self.values.get(index) {
            Some(value) => value.as_ref(),
            None => None,
        }
    }

    pub fn get_mut(&mut self, sequence: u32) -> Option<&mut T> {
        let index = self.sequence_to_index(sequence);
        match self.values.get_mut(index) {
            Some(value) => value.as_mut(),
            None => None,
        }
    }
}
