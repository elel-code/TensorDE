use bevy_ecs::entity::Entity;

use crate::Entry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CachedEntryMatch {
    Exact,
    Equivalent,
    Mismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CachedVisibleItemSlot {
    pub(super) entry_index: Option<usize>,
    pub(super) entry: Option<Entry>,
    pub(super) entity: Option<Entity>,
    pub(super) slot_id: u64,
}

impl CachedVisibleItemSlot {
    pub(super) fn from_entry_index(
        entry_index: Option<usize>,
        entries: &[Entry],
        entity: Option<Entity>,
        slot_id: u64,
    ) -> Self {
        let entry = entry_index.and_then(|index| entries.get(index)).cloned();
        Self {
            entry_index,
            entry,
            entity,
            slot_id,
        }
    }

    pub(super) fn classify(
        &self,
        entry_index: Option<usize>,
        entry: Option<&Entry>,
    ) -> CachedEntryMatch {
        if self.entry_index != entry_index {
            return CachedEntryMatch::Mismatch;
        }
        match (self.entry.as_ref(), entry) {
            (Some(cached), Some(entry)) if Entry::ptr_eq(cached, entry) => CachedEntryMatch::Exact,
            (Some(cached), Some(entry)) if same_item_identity(cached, entry) => {
                CachedEntryMatch::Equivalent
            }
            (None, None) => CachedEntryMatch::Exact,
            _ => CachedEntryMatch::Mismatch,
        }
    }

    pub(super) fn refresh_entry(&mut self, entry: &Entry) {
        self.entry = Some(entry.clone());
    }
}

fn same_item_identity(cached: &Entry, current: &Entry) -> bool {
    match (
        cached.target_path.as_deref(),
        current.target_path.as_deref(),
    ) {
        (None, None) => cached.name == current.name,
        (Some(cached), Some(current)) => cached == current,
        _ => false,
    }
}
