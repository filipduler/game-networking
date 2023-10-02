pub struct SequenceBuffer<T> {
    values: Vec<Option<T>>,
    pub partition_by: u16,
}

impl<T> SequenceBuffer<T> {
    pub fn with_size(size: u16) -> Self {
        SequenceBuffer {
            values: (0..size).map(|_| None::<T>).collect(),
            partition_by: size,
        }
    }

    pub fn sequence_to_index(&self, sequence: u16) -> usize {
        (sequence % self.partition_by) as usize
    }

    pub fn insert(&mut self, sequence: u16, value: T) -> Option<&mut T> {
        let index = self.sequence_to_index(sequence);
        self.values[index] = Some(value);
        self.values[index].as_mut()
    }

    pub fn remove(&mut self, sequence: u16) {
        let index = self.sequence_to_index(sequence);
        self.values[index] = None;
    }

    pub fn is_some(&self, sequence: u16) -> bool {
        let index = self.sequence_to_index(sequence);
        self.values[index].is_some()
    }

    pub fn is_none(&self, sequence: u16) -> bool {
        let index = self.sequence_to_index(sequence);
        self.values[index].is_none()
    }

    pub fn take(&mut self, sequence: u16) -> Option<T> {
        let index = self.sequence_to_index(sequence);
        self.values[index].take()
    }

    pub fn get(&self, sequence: u16) -> Option<&T> {
        let index = self.sequence_to_index(sequence);
        match self.values.get(index) {
            Some(value) => value.as_ref(),
            None => None,
        }
    }

    pub fn get_mut(&mut self, sequence: u16) -> Option<&mut T> {
        let index = self.sequence_to_index(sequence);
        match self.values.get_mut(index) {
            Some(value) => value.as_mut(),
            None => None,
        }
    }
}
