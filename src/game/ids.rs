pub type ID = u32;

pub const NO_ID: ID = 0;

/// Class that always returns unique id
pub struct IdProducer {
    last_issued_id: ID,
}

impl IdProducer {
    pub fn new() -> Self {
        Self {
            last_issued_id: NO_ID,
        }
    }

    pub fn next(&mut self) -> ID {
        self.last_issued_id += 1;
        self.last_issued_id
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::{IdProducer, ID, NO_ID};

    #[test]
    fn check_producer_has_no_duplicates() {
        let mut used_values: HashSet<ID> = HashSet::default();
        let mut producer = IdProducer::new();

        let size: usize = 100000;
        for _ in 1..size {
            used_values.insert(producer.next());
        }
        assert_eq!(used_values.len(), size - 1);
        assert!(!used_values.contains(&NO_ID));
    }
}
