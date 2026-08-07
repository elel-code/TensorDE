use std::collections::HashMap;
use std::sync::Arc;

use tensor_files_core::{format_modified_secs, format_size};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DetailsTextCacheStats {
    pub(crate) entries: usize,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DetailsTextKind {
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DetailsTextKey {
    kind: DetailsTextKind,
    is_dir: bool,
    metadata_complete: bool,
    size_bytes: u64,
    modified_secs: Option<u64>,
}

#[derive(Debug)]
struct CachedDetailsText {
    text: Arc<str>,
    last_used_generation: u64,
}

#[derive(Debug)]
pub(crate) struct DetailsTextCache {
    entries: HashMap<DetailsTextKey, CachedDetailsText>,
    eviction_generations: Vec<u64>,
    max_entries: usize,
    generation: u64,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl DetailsTextCache {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            eviction_generations: Vec::new(),
            max_entries: max_entries.max(1),
            generation: 0,
            #[cfg(test)]
            hits: 0,
            #[cfg(test)]
            misses: 0,
        }
    }

    pub(crate) fn size_label(
        &mut self,
        is_dir: bool,
        metadata_complete: bool,
        size_bytes: u64,
        modified_secs: Option<u64>,
    ) -> Arc<str> {
        let key = DetailsTextKey {
            kind: DetailsTextKind::Size,
            is_dir,
            metadata_complete,
            size_bytes,
            modified_secs,
        };
        self.get_or_insert(key, || {
            if is_dir {
                Arc::from("Folder")
            } else if !metadata_complete && size_bytes == 0 && modified_secs.is_none() {
                Arc::from("-")
            } else {
                Arc::from(format_size(size_bytes))
            }
        })
    }

    pub(crate) fn modified_label(&mut self, modified_secs: Option<u64>) -> Arc<str> {
        let key = DetailsTextKey {
            kind: DetailsTextKind::Modified,
            is_dir: false,
            metadata_complete: true,
            size_bytes: 0,
            modified_secs,
        };
        self.get_or_insert(key, || Arc::from(format_modified_secs(modified_secs)))
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> DetailsTextCacheStats {
        DetailsTextCacheStats {
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
        }
    }

    fn get_or_insert(&mut self, key: DetailsTextKey, build: impl FnOnce() -> Arc<str>) -> Arc<str> {
        let generation = self.next_generation();
        if let Some(cached) = self.entries.get_mut(&key) {
            cached.last_used_generation = generation;
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return Arc::clone(&cached.text);
        }

        #[cfg(test)]
        {
            self.misses += 1;
        }
        let text = build();
        self.entries.insert(
            key,
            CachedDetailsText {
                text: Arc::clone(&text),
                last_used_generation: generation,
            },
        );
        self.prune();
        text
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn prune(&mut self) {
        if self.entries.len() <= self.max_entries {
            return;
        }
        let retain_count = self.max_entries.saturating_mul(3).div_ceil(4).max(1);
        self.eviction_generations.clear();
        self.eviction_generations.extend(
            self.entries
                .values()
                .map(|entry| entry.last_used_generation),
        );
        let cutoff_index = self.eviction_generations.len() - retain_count - 1;
        let (_, cutoff, _) = self.eviction_generations.select_nth_unstable(cutoff_index);
        let cutoff = *cutoff;
        self.entries
            .retain(|_, entry| entry.last_used_generation > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_labels_reuse_text_and_metadata_changes_miss() {
        let mut cache = DetailsTextCache::new(4);
        let first_size = cache.size_label(false, true, 1536, Some(42));
        let second_size = cache.size_label(false, true, 1536, Some(42));
        let changed_size = cache.size_label(false, true, 2048, Some(42));
        let first_modified = cache.modified_label(Some(42));
        let second_modified = cache.modified_label(Some(42));

        assert!(Arc::ptr_eq(&first_size, &second_size));
        assert!(!Arc::ptr_eq(&first_size, &changed_size));
        assert!(Arc::ptr_eq(&first_modified, &second_modified));
        assert_eq!(first_size.as_ref(), "1.5 KB");
        assert_eq!(first_modified.as_ref(), "1970-01-01 00:00");
        assert_eq!(
            cache.stats(),
            DetailsTextCacheStats {
                entries: 3,
                hits: 2,
                misses: 3,
            }
        );
    }

    #[test]
    fn cache_prunes_oldest_batch_without_unbounded_growth() {
        let mut cache = DetailsTextCache::new(4);
        for size in 0..4 {
            cache.size_label(false, true, size, Some(size));
        }
        cache.size_label(false, true, 0, Some(0));
        cache.size_label(false, true, 4, Some(4));

        assert_eq!(cache.stats().entries, 3);
        assert_eq!(cache.size_label(false, true, 1, Some(1)).as_ref(), "1 B");
        assert_eq!(cache.stats().entries, 4);
    }
}
