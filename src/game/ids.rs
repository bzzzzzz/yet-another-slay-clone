pub type ID = u32;

/// Class that always returns unique id
pub struct IdProducer {
    last_issued_id: ID,
}

impl IdProducer {
    pub fn new() -> Self {
        Self {last_issued_id: 0,}
    }

    pub fn next(&mut self) -> ID {
        self.last_issued_id += 1;
        self.last_issued_id
    }
}
