use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LabelTextInternerStats {
    pub(crate) entries: usize,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
}

#[derive(Debug)]
struct InternedLabelText {
    last_used_generation: Cell<u64>,
}

#[derive(Debug)]
pub(crate) struct LabelTextInterner {
    entries: HashMap<Arc<str>, InternedLabelText>,
    eviction_generations: Vec<u64>,
    max_entries: usize,
    generation: u64,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl LabelTextInterner {
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

    pub(crate) fn intern(&mut self, label: &str) -> Arc<str> {
        let generation = self.next_generation();
        let cached = self.entries.get_key_value(label).map(|(text, entry)| {
            entry.last_used_generation.set(generation);
            Arc::clone(text)
        });
        if let Some(text) = cached {
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return text;
        }

        #[cfg(test)]
        {
            self.misses += 1;
        }
        let text = Arc::<str>::from(label);
        self.entries.insert(
            Arc::clone(&text),
            InternedLabelText {
                last_used_generation: Cell::new(generation),
            },
        );
        self.prune();
        text
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> LabelTextInternerStats {
        LabelTextInternerStats {
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
        }
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
    fn warm_lookup_reuses_the_owned_label() {
        let mut interner = LabelTextInterner::new(4);
        let first = interner.intern("alpha.txt");
        let second = interner.intern("alpha.txt");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            interner.stats(),
            LabelTextInternerStats {
                entries: 1,
                hits: 1,
                misses: 1,
            }
        );
    }

    #[test]
    fn interner_prunes_the_oldest_name_batch() {
        let mut interner = LabelTextInterner::new(4);
        let alpha = interner.intern("alpha");
        let beta = interner.intern("beta");
        interner.intern("gamma");
        interner.intern("delta");
        interner.intern("alpha");
        interner.intern("epsilon");

        assert!(Arc::ptr_eq(&alpha, &interner.intern("alpha")));
        assert_eq!(interner.stats().entries, 3);
        assert!(!Arc::ptr_eq(&beta, &interner.intern("beta")));
        assert_eq!(interner.stats().entries, 4);
    }
}
