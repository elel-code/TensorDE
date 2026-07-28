use tensor_event::{BackendInputEvent, DeviceId, TabletToolId};

const MAX_PENDING_EVENTS: usize = 12;
const MAX_TOOLS: usize = 64;

pub(super) struct PendingEvents {
    slots: [Option<BackendInputEvent>; MAX_PENDING_EVENTS],
    head: usize,
    len: usize,
}

impl PendingEvents {
    pub(super) const fn new() -> Self {
        Self {
            slots: [None; MAX_PENDING_EVENTS],
            head: 0,
            len: 0,
        }
    }

    pub(super) fn push(&mut self, event: BackendInputEvent) -> bool {
        if self.len == self.slots.len() {
            return false;
        }
        let tail = (self.head + self.len) % self.slots.len();
        self.slots[tail] = Some(event);
        self.len += 1;
        true
    }

    pub(super) fn can_push(&self, count: usize) -> bool {
        count <= self.slots.len() - self.len
    }

    pub(super) fn pop(&mut self) -> Option<BackendInputEvent> {
        if self.len == 0 {
            return None;
        }
        let event = self.slots[self.head].take();
        self.head = (self.head + 1) % self.slots.len();
        self.len -= 1;
        event
    }
}

struct ToolEntry {
    raw: usize,
    id: TabletToolId,
    device: DeviceId,
}

pub(super) struct ToolRegistry {
    entries: Vec<ToolEntry>,
    next_id: u64,
}

impl ToolRegistry {
    pub(super) fn new() -> Self {
        Self {
            entries: Vec::with_capacity(MAX_TOOLS),
            next_id: 1,
        }
    }

    pub(super) fn id_for(&mut self, raw: usize, device: DeviceId) -> Option<(TabletToolId, bool)> {
        if let Some(entry) = self.entries.iter().find(|entry| entry.raw == raw) {
            return Some((entry.id, false));
        }
        if self.entries.len() == MAX_TOOLS {
            tracing::warn!("tablet tool capacity exceeded");
            return None;
        }
        let id = TabletToolId::new(self.next_id);
        self.next_id = self.next_id.checked_add(1).or_else(|| {
            tracing::error!("tablet tool identity space exhausted");
            None
        })?;
        self.entries.push(ToolEntry { raw, id, device });
        Some((id, true))
    }

    pub(super) fn remove_device(&mut self, device: DeviceId) {
        self.entries.retain(|entry| entry.device != device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn key(key: u32) -> BackendInputEvent {
        BackendInputEvent::Keyboard(tensor_event::KeyboardEvent {
            key,
            pressed: true,
            time_ns: 0,
        })
    }

    #[test]
    fn pending_ring_preserves_capacity_across_wrap() {
        let mut pending = PendingEvents::new();
        for code in 0..MAX_PENDING_EVENTS as u32 {
            assert!(pending.push(key(code)));
        }
        assert!(!pending.push(key(100)));
        for expected in 0..6 {
            assert!(
                matches!(pending.pop(), Some(BackendInputEvent::Keyboard(event)) if event.key == expected)
            );
        }
        for code in MAX_PENDING_EVENTS as u32..MAX_PENDING_EVENTS as u32 + 6 {
            assert!(pending.push(key(code)));
        }
        for expected in 6..MAX_PENDING_EVENTS as u32 + 6 {
            assert!(
                matches!(pending.pop(), Some(BackendInputEvent::Keyboard(event)) if event.key == expected)
            );
        }
        assert!(pending.pop().is_none());
    }

    #[test]
    fn tool_registry_releases_capacity_without_reusing_ids() {
        let mut tools = ToolRegistry::new();
        let first_device = DeviceId::new(1);
        let second_device = DeviceId::new(2);
        let (first, added) = tools.id_for(10, first_device).unwrap();
        assert!(added);
        assert_eq!(tools.id_for(10, first_device), Some((first, false)));
        for raw in 1..MAX_TOOLS {
            tools.id_for(10 + raw, first_device).unwrap();
        }
        assert!(tools.id_for(10_000, second_device).is_none());

        tools.remove_device(first_device);
        let (replacement, added) = tools.id_for(10_000, second_device).unwrap();
        assert!(added);
        assert_ne!(replacement, first);
    }
}
