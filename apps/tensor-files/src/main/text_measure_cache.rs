use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
struct CachedNoWrapMeasure {
    font_size_bits: u32,
    line_height_bits: u32,
    width: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct CachedIconsLineMeasure {
    available_width_bits: u32,
    max_lines: usize,
    font_size_bits: u32,
    line_height_bits: u32,
    lines: usize,
}

#[derive(Clone, Debug, Default)]
struct CachedTextMeasures {
    no_wrap: Option<CachedNoWrapMeasure>,
    icons_lines: Option<CachedIconsLineMeasure>,
    details_display: Option<CachedFilenameDisplay>,
    icons_display: Option<CachedFilenameDisplay>,
    last_used_generation: u64,
}

#[derive(Clone, Debug)]
struct CachedFilenameDisplay {
    available_width_bits: u32,
    max_lines: usize,
    font_size_bits: u32,
    line_height_bits: u32,
    display: Arc<str>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextMeasureCacheStats {
    pub(crate) entries: usize,
    pub(crate) no_wrap_hits: usize,
    pub(crate) no_wrap_misses: usize,
    pub(crate) icons_hits: usize,
    pub(crate) icons_misses: usize,
    pub(crate) details_display_hits: usize,
    pub(crate) details_display_misses: usize,
    pub(crate) icons_display_hits: usize,
    pub(crate) icons_display_misses: usize,
}

pub(crate) struct TextMeasureCache {
    entries: HashMap<Arc<str>, CachedTextMeasures>,
    eviction_generations: Vec<u64>,
    max_entries: usize,
    generation: u64,
    #[cfg(test)]
    no_wrap_hits: usize,
    #[cfg(test)]
    no_wrap_misses: usize,
    #[cfg(test)]
    icons_hits: usize,
    #[cfg(test)]
    icons_misses: usize,
    #[cfg(test)]
    details_display_hits: usize,
    #[cfg(test)]
    details_display_misses: usize,
    #[cfg(test)]
    icons_display_hits: usize,
    #[cfg(test)]
    icons_display_misses: usize,
}

impl TextMeasureCache {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            eviction_generations: Vec::new(),
            max_entries: max_entries.max(1),
            generation: 0,
            #[cfg(test)]
            no_wrap_hits: 0,
            #[cfg(test)]
            no_wrap_misses: 0,
            #[cfg(test)]
            icons_hits: 0,
            #[cfg(test)]
            icons_misses: 0,
            #[cfg(test)]
            details_display_hits: 0,
            #[cfg(test)]
            details_display_misses: 0,
            #[cfg(test)]
            icons_display_hits: 0,
            #[cfg(test)]
            icons_display_misses: 0,
        }
    }

    pub(crate) fn no_wrap_width(
        &mut self,
        label: &str,
        font_size: f32,
        line_height: f32,
    ) -> Option<f32> {
        let generation = self.next_generation();
        let Some(cached) = self.entries.get_mut(label) else {
            #[cfg(test)]
            {
                self.no_wrap_misses += 1;
            }
            return None;
        };
        let Some(measure) = cached.no_wrap.filter(|measure| {
            measure.font_size_bits == font_size.to_bits()
                && measure.line_height_bits == line_height.to_bits()
        }) else {
            #[cfg(test)]
            {
                self.no_wrap_misses += 1;
            }
            return None;
        };
        cached.last_used_generation = generation;
        #[cfg(test)]
        {
            self.no_wrap_hits += 1;
        }
        Some(measure.width)
    }

    pub(crate) fn insert_no_wrap_width(
        &mut self,
        label: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
    ) {
        let generation = self.next_generation();
        if let Some(cached) = self.entries.get_mut(label) {
            cached.no_wrap = Some(CachedNoWrapMeasure {
                font_size_bits: font_size.to_bits(),
                line_height_bits: line_height.to_bits(),
                width,
            });
            cached.last_used_generation = generation;
            return;
        }
        self.entries.insert(
            Arc::<str>::from(label),
            CachedTextMeasures {
                no_wrap: Some(CachedNoWrapMeasure {
                    font_size_bits: font_size.to_bits(),
                    line_height_bits: line_height.to_bits(),
                    width,
                }),
                icons_lines: None,
                details_display: None,
                icons_display: None,
                last_used_generation: generation,
            },
        );
        self.prune();
    }

    pub(crate) fn icons_line_count(
        &mut self,
        label: &str,
        available_width: f32,
        max_lines: usize,
        font_size: f32,
        line_height: f32,
    ) -> Option<usize> {
        let generation = self.next_generation();
        let Some(cached) = self.entries.get_mut(label) else {
            #[cfg(test)]
            {
                self.icons_misses += 1;
            }
            return None;
        };
        let Some(measure) = cached.icons_lines.filter(|measure| {
            measure.available_width_bits == available_width.to_bits()
                && measure.max_lines == max_lines
                && measure.font_size_bits == font_size.to_bits()
                && measure.line_height_bits == line_height.to_bits()
        }) else {
            #[cfg(test)]
            {
                self.icons_misses += 1;
            }
            return None;
        };
        cached.last_used_generation = generation;
        #[cfg(test)]
        {
            self.icons_hits += 1;
        }
        Some(measure.lines)
    }

    pub(crate) fn insert_icons_line_count(
        &mut self,
        label: &str,
        available_width: f32,
        max_lines: usize,
        font_size: f32,
        line_height: f32,
        lines: usize,
    ) {
        let generation = self.next_generation();
        if let Some(cached) = self.entries.get_mut(label) {
            cached.icons_lines = Some(CachedIconsLineMeasure {
                available_width_bits: available_width.to_bits(),
                max_lines,
                font_size_bits: font_size.to_bits(),
                line_height_bits: line_height.to_bits(),
                lines,
            });
            cached.last_used_generation = generation;
            return;
        }
        self.entries.insert(
            Arc::<str>::from(label),
            CachedTextMeasures {
                no_wrap: None,
                icons_lines: Some(CachedIconsLineMeasure {
                    available_width_bits: available_width.to_bits(),
                    max_lines,
                    font_size_bits: font_size.to_bits(),
                    line_height_bits: line_height.to_bits(),
                    lines,
                }),
                details_display: None,
                icons_display: None,
                last_used_generation: generation,
            },
        );
        self.prune();
    }

    pub(crate) fn details_filename_display(
        &mut self,
        label: &str,
        available_width: f32,
        font_size: f32,
        line_height: f32,
    ) -> Option<Arc<str>> {
        let generation = self.next_generation();
        let Some(cached) = self.entries.get_mut(label) else {
            #[cfg(test)]
            {
                self.details_display_misses += 1;
            }
            return None;
        };
        let Some(display) = cached.details_display.as_ref().filter(|display| {
            display.available_width_bits == available_width.to_bits()
                && display.max_lines == 1
                && display.font_size_bits == font_size.to_bits()
                && display.line_height_bits == line_height.to_bits()
        }) else {
            #[cfg(test)]
            {
                self.details_display_misses += 1;
            }
            return None;
        };
        cached.last_used_generation = generation;
        #[cfg(test)]
        {
            self.details_display_hits += 1;
        }
        Some(Arc::clone(&display.display))
    }

    pub(crate) fn insert_details_filename_display(
        &mut self,
        label: &str,
        available_width: f32,
        font_size: f32,
        line_height: f32,
        display: Arc<str>,
    ) {
        let generation = self.next_generation();
        if let Some(cached) = self.entries.get_mut(label) {
            cached.details_display = Some(CachedFilenameDisplay {
                available_width_bits: available_width.to_bits(),
                max_lines: 1,
                font_size_bits: font_size.to_bits(),
                line_height_bits: line_height.to_bits(),
                display,
            });
            cached.last_used_generation = generation;
            return;
        }
        self.entries.insert(
            Arc::<str>::from(label),
            CachedTextMeasures {
                no_wrap: None,
                icons_lines: None,
                details_display: Some(CachedFilenameDisplay {
                    available_width_bits: available_width.to_bits(),
                    max_lines: 1,
                    font_size_bits: font_size.to_bits(),
                    line_height_bits: line_height.to_bits(),
                    display,
                }),
                icons_display: None,
                last_used_generation: generation,
            },
        );
        self.prune();
    }

    pub(crate) fn icons_filename_display(
        &mut self,
        label: &str,
        available_width: f32,
        max_lines: usize,
        font_size: f32,
        line_height: f32,
    ) -> Option<Arc<str>> {
        let generation = self.next_generation();
        let Some(cached) = self.entries.get_mut(label) else {
            #[cfg(test)]
            {
                self.icons_display_misses += 1;
            }
            return None;
        };
        let Some(display) = cached.icons_display.as_ref().filter(|display| {
            display.available_width_bits == available_width.to_bits()
                && display.max_lines == max_lines
                && display.font_size_bits == font_size.to_bits()
                && display.line_height_bits == line_height.to_bits()
        }) else {
            #[cfg(test)]
            {
                self.icons_display_misses += 1;
            }
            return None;
        };
        cached.last_used_generation = generation;
        #[cfg(test)]
        {
            self.icons_display_hits += 1;
        }
        Some(Arc::clone(&display.display))
    }

    pub(crate) fn insert_icons_filename_display(
        &mut self,
        label: &str,
        available_width: f32,
        max_lines: usize,
        font_size: f32,
        line_height: f32,
        display: Arc<str>,
    ) {
        let generation = self.next_generation();
        if let Some(cached) = self.entries.get_mut(label) {
            cached.icons_display = Some(CachedFilenameDisplay {
                available_width_bits: available_width.to_bits(),
                max_lines,
                font_size_bits: font_size.to_bits(),
                line_height_bits: line_height.to_bits(),
                display,
            });
            cached.last_used_generation = generation;
            return;
        }
        self.entries.insert(
            Arc::<str>::from(label),
            CachedTextMeasures {
                no_wrap: None,
                icons_lines: None,
                details_display: None,
                icons_display: Some(CachedFilenameDisplay {
                    available_width_bits: available_width.to_bits(),
                    max_lines,
                    font_size_bits: font_size.to_bits(),
                    line_height_bits: line_height.to_bits(),
                    display,
                }),
                last_used_generation: generation,
            },
        );
        self.prune();
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> TextMeasureCacheStats {
        TextMeasureCacheStats {
            entries: self.entries.len(),
            no_wrap_hits: self.no_wrap_hits,
            no_wrap_misses: self.no_wrap_misses,
            icons_hits: self.icons_hits,
            icons_misses: self.icons_misses,
            details_display_hits: self.details_display_hits,
            details_display_misses: self.details_display_misses,
            icons_display_hits: self.icons_display_hits,
            icons_display_misses: self.icons_display_misses,
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
                .map(|cached| cached.last_used_generation),
        );
        let cutoff_index = self.eviction_generations.len() - retain_count - 1;
        let (_, cutoff, _) = self.eviction_generations.select_nth_unstable(cutoff_index);
        let cutoff = *cutoff;
        self.entries
            .retain(|_, cached| cached.last_used_generation > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_label_and_style_reuses_both_metric_kinds() {
        let mut cache = TextMeasureCache::new(4);
        cache.insert_no_wrap_width("alpha.txt", 12.0, 18.0, 71.5);
        cache.insert_icons_line_count("alpha.txt", 80.0, 3, 12.0, 18.0, 2);

        assert_eq!(cache.no_wrap_width("alpha.txt", 12.0, 18.0), Some(71.5));
        assert_eq!(
            cache.icons_line_count("alpha.txt", 80.0, 3, 12.0, 18.0),
            Some(2)
        );
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().no_wrap_hits, 1);
        assert_eq!(cache.stats().icons_hits, 1);
    }

    #[test]
    fn changed_measurement_style_misses_without_growing_name_storage() {
        let mut cache = TextMeasureCache::new(4);
        cache.insert_no_wrap_width("alpha.txt", 12.0, 18.0, 71.5);
        cache.insert_icons_line_count("alpha.txt", 80.0, 3, 12.0, 18.0, 2);

        assert_eq!(cache.no_wrap_width("alpha.txt", 13.0, 18.0), None);
        assert_eq!(
            cache.icons_line_count("alpha.txt", 72.0, 3, 12.0, 18.0),
            None
        );
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().no_wrap_misses, 1);
        assert_eq!(cache.stats().icons_misses, 1);
    }

    #[test]
    fn filename_displays_reuse_owned_text_and_invalidate_by_style() {
        let mut cache = TextMeasureCache::new(4);
        let details = Arc::<str>::from("alpha….txt");
        let icons = Arc::<str>::from("alpha\u{200b}.txt");
        cache.insert_details_filename_display(
            "alpha-long.txt",
            80.0,
            12.0,
            18.0,
            Arc::clone(&details),
        );
        cache.insert_icons_filename_display(
            "alpha-long.txt",
            80.0,
            3,
            12.0,
            18.0,
            Arc::clone(&icons),
        );

        let details_hit = cache
            .details_filename_display("alpha-long.txt", 80.0, 12.0, 18.0)
            .unwrap();
        let icons_hit = cache
            .icons_filename_display("alpha-long.txt", 80.0, 3, 12.0, 18.0)
            .unwrap();
        assert!(Arc::ptr_eq(&details, &details_hit));
        assert!(Arc::ptr_eq(&icons, &icons_hit));
        assert_eq!(
            cache.details_filename_display("alpha-long.txt", 79.0, 12.0, 18.0),
            None
        );
        assert_eq!(
            cache.icons_filename_display("alpha-long.txt", 80.0, 2, 12.0, 18.0),
            None
        );
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().details_display_hits, 1);
        assert_eq!(cache.stats().details_display_misses, 1);
        assert_eq!(cache.stats().icons_display_hits, 1);
        assert_eq!(cache.stats().icons_display_misses, 1);
    }

    #[test]
    fn metric_cache_prunes_the_oldest_name_batch() {
        let mut cache = TextMeasureCache::new(4);
        cache.insert_no_wrap_width("alpha.txt", 12.0, 18.0, 1.0);
        cache.insert_no_wrap_width("beta.txt", 12.0, 18.0, 2.0);
        cache.insert_no_wrap_width("gamma.txt", 12.0, 18.0, 3.0);
        cache.insert_no_wrap_width("delta.txt", 12.0, 18.0, 4.0);
        assert_eq!(cache.no_wrap_width("alpha.txt", 12.0, 18.0), Some(1.0));
        cache.insert_no_wrap_width("epsilon.txt", 12.0, 18.0, 5.0);

        assert_eq!(cache.no_wrap_width("alpha.txt", 12.0, 18.0), Some(1.0));
        assert_eq!(cache.no_wrap_width("beta.txt", 12.0, 18.0), None);
        assert_eq!(cache.no_wrap_width("gamma.txt", 12.0, 18.0), None);
        assert_eq!(cache.no_wrap_width("delta.txt", 12.0, 18.0), Some(4.0));
        assert_eq!(cache.no_wrap_width("epsilon.txt", 12.0, 18.0), Some(5.0));
        assert_eq!(cache.stats().entries, 3);
        assert_eq!(cache.stats().no_wrap_hits, 4);
        assert_eq!(cache.stats().no_wrap_misses, 2);
    }
}
