use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_MAX_ENTRIES: usize = 512;

#[derive(Debug)]
struct InternedIconName {
    last_used_generation: Cell<u64>,
}

#[derive(Debug)]
pub(crate) struct IconNameInterner {
    entries: HashMap<Arc<str>, InternedIconName>,
    eviction_generations: Vec<u64>,
    max_entries: usize,
    generation: u64,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl Default for IconNameInterner {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }
}

impl IconNameInterner {
    fn new(max_entries: usize) -> Self {
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

    pub(crate) fn intern(&mut self, name: &str) -> Arc<str> {
        let generation = self.next_generation();
        if let Some((name, cached)) = self.entries.get_key_value(name) {
            cached.last_used_generation.set(generation);
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return Arc::clone(name);
        }

        #[cfg(test)]
        {
            self.misses += 1;
        }
        let name = Arc::<str>::from(name);
        self.entries.insert(
            Arc::clone(&name),
            InternedIconName {
                last_used_generation: Cell::new(generation),
            },
        );
        self.prune();
        name
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
                .map(|entry| entry.last_used_generation.get()),
        );
        let cutoff_index = self.eviction_generations.len() - retain_count - 1;
        let (_, cutoff, _) = self.eviction_generations.select_nth_unstable(cutoff_index);
        let cutoff = *cutoff;
        self.entries
            .retain(|_, entry| entry.last_used_generation.get() > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_lookup_reuses_the_interned_name() {
        let mut interner = IconNameInterner::new(4);
        let first = interner.intern("emblem-readonly");
        let second = interner.intern("emblem-readonly");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(interner.entries.len(), 1);
        assert_eq!((interner.hits, interner.misses), (1, 1));
    }

    #[test]
    fn interner_prunes_the_oldest_name_batch() {
        let mut interner = IconNameInterner::new(4);
        let oldest = interner.intern("oldest");
        for name in ["second", "third", "fourth", "newest"] {
            interner.intern(name);
        }

        assert!(interner.entries.len() <= 4);
        assert!(!interner.entries.contains_key(oldest.as_ref()));
        assert!(interner.entries.contains_key("newest"));
    }
}
