use std::collections::{HashSet, VecDeque};

use crate::ui::icon_roles::FileIconPathCacheKey;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct IconRoleReadAheadRequest {
    pub(crate) key: FileIconPathCacheKey,
}

pub(crate) struct ShellIconRoleReadAheadQueue {
    queue: VecDeque<IconRoleReadAheadRequest>,
    seen: HashSet<IconRoleReadAheadRequest>,
}

impl ShellIconRoleReadAheadQueue {
    pub(crate) fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            seen: HashSet::new(),
        }
    }

    pub(crate) fn push_key(&mut self, key: FileIconPathCacheKey) {
        let request = IconRoleReadAheadRequest { key };
        if self.seen.insert(request.clone()) {
            self.queue.push_back(request);
        }
    }

    pub(crate) fn pop_front(&mut self) -> Option<IconRoleReadAheadRequest> {
        let request = self.queue.pop_front()?;
        self.seen.remove(&request);
        Some(request)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icon_roles::{FileIconKind, FileIconRoleCacheKey};

    fn directory_key(size_px: u16) -> FileIconPathCacheKey {
        FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Directory,
            },
            size_px,
        }
    }

    #[test]
    fn icon_role_read_ahead_queue_dedupes_and_preserves_order() {
        let mut queue = ShellIconRoleReadAheadQueue::new();
        let first = directory_key(32);
        let second = directory_key(48);

        queue.push_key(first.clone());
        queue.push_key(second.clone());
        queue.push_key(first.clone());

        assert_eq!(queue.pop_front().map(|request| request.key), Some(first));
        assert_eq!(queue.pop_front().map(|request| request.key), Some(second));
        assert!(queue.pop_front().is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn icon_role_read_ahead_queue_allows_requeue_after_pop() {
        let mut queue = ShellIconRoleReadAheadQueue::new();
        let key = directory_key(32);

        queue.push_key(key.clone());
        assert_eq!(
            queue.pop_front().map(|request| request.key),
            Some(key.clone())
        );

        queue.push_key(key.clone());

        assert_eq!(queue.pop_front().map(|request| request.key), Some(key));
        assert!(queue.is_empty());
    }
}
