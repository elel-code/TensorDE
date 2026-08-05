const MAX_PENDING_CONFIGURES: usize = 16;

#[derive(Debug)]
pub(super) struct ConfigureQueue<T: Copy> {
    entries: [Option<T>; MAX_PENDING_CONFIGURES],
    serials: [u32; MAX_PENDING_CONFIGURES],
    head: usize,
    len: usize,
}

impl<T: Copy> ConfigureQueue<T> {
    pub(super) fn new() -> Self {
        Self {
            entries: [None; MAX_PENDING_CONFIGURES],
            serials: [0; MAX_PENDING_CONFIGURES],
            head: 0,
            len: 0,
        }
    }

    pub(super) fn push(&mut self, serial: u32, value: T) {
        if self.len == MAX_PENDING_CONFIGURES {
            self.entries[self.head] = None;
            self.head = (self.head + 1) % MAX_PENDING_CONFIGURES;
            self.len -= 1;
        }
        let tail = (self.head + self.len) % MAX_PENDING_CONFIGURES;
        self.serials[tail] = serial;
        self.entries[tail] = Some(value);
        self.len += 1;
    }

    pub(super) fn ack(&mut self, serial: u32) -> Option<T> {
        let offset = (0..self.len).find(|offset| {
            let index = (self.head + offset) % MAX_PENDING_CONFIGURES;
            self.serials[index] == serial && self.entries[index].is_some()
        })?;
        let value = self.entries[(self.head + offset) % MAX_PENDING_CONFIGURES]?;
        for _ in 0..=offset {
            self.entries[self.head] = None;
            self.head = (self.head + 1) % MAX_PENDING_CONFIGURES;
            self.len -= 1;
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_fixed_and_ack_consumes_older_entries() {
        let mut queue = ConfigureQueue::new();
        for serial in 1..=(MAX_PENDING_CONFIGURES as u32 + 1) {
            queue.push(serial, serial);
        }
        assert_eq!(queue.len, MAX_PENDING_CONFIGURES);
        assert_eq!(queue.ack(1), None);
        assert_eq!(queue.ack(3), Some(3));
        assert_eq!(queue.len, MAX_PENDING_CONFIGURES - 2);
    }
}
