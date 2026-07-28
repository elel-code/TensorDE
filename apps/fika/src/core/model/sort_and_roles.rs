fn reusable_thumbnail_path(old: &ModelEntry, new: &ModelEntry) -> Option<PathBuf> {
    if !thumbnail_path_can_be_reused(old, new) {
        None
    } else {
        old.thumbnail_path.clone()
    }
}

fn reusable_thumbnail_failed(old: &ModelEntry, new: &ModelEntry) -> bool {
    thumbnail_failed_can_be_reused(old, new) && old.thumbnail_failed
}

fn thumbnail_path_can_be_reused(old: &ModelEntry, new: &ModelEntry) -> bool {
    thumbnail_base_can_be_reused(old, new)
}

fn thumbnail_failed_can_be_reused(old: &ModelEntry, new: &ModelEntry) -> bool {
    thumbnail_base_can_be_reused(old, new) && old.effective_mime_type() == new.effective_mime_type()
}

fn thumbnail_base_can_be_reused(old: &ModelEntry, new: &ModelEntry) -> bool {
    !(old.is_dir
        || new.is_dir
        || old.name != new.name
        || !old.effective_metadata_complete()
        || !new.effective_metadata_complete()
        || old.effective_size_bytes() != new.effective_size_bytes()
        || old.effective_modified_secs().is_none()
        || old.effective_modified_secs() != new.effective_modified_secs())
}

fn preserve_refreshed_entry_roles(old: &ModelEntry, new: &mut ModelEntry) {
    preserve_pending_entry_metadata_role(old, new);
    new.thumbnail_path = reusable_thumbnail_path(old, new);
    new.thumbnail_failed = reusable_thumbnail_failed(old, new);
}

fn preserve_pending_entry_metadata_role(old: &ModelEntry, new: &mut ModelEntry) {
    if old.name != new.name
        || old.is_dir != new.is_dir
        || !old.effective_metadata_complete()
        || !new_requires_async_metadata_role(new)
    {
        return;
    }

    let old_role = old.effective_metadata_role();
    if !old_role.mime_magic_checked {
        return;
    }

    let role = base_metadata_role(new);
    if old_role.size_bytes == role.size_bytes && old_role.modified_secs == role.modified_secs {
        new.metadata_role = (!metadata_role_matches_base_entry(new, &old_role)).then_some(old_role);
    }
}

fn new_requires_async_metadata_role(entry: &ModelEntry) -> bool {
    !entry.is_dir
        && (!entry.entry.metadata_complete
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.entry.size_bytes,
                entry.entry.mime_type.as_deref(),
                entry.entry.mime_magic_checked,
            ))
}

fn base_metadata_role(entry: &ModelEntry) -> EntryMetadataRole {
    EntryMetadataRole {
        size_bytes: entry.entry.size_bytes,
        modified_secs: entry.entry.modified_secs,
        mime_type: entry.entry.mime_type.clone(),
        mime_magic_checked: entry.entry.mime_magic_checked,
    }
}

fn metadata_role_matches_base_entry(entry: &ModelEntry, role: &EntryMetadataRole) -> bool {
    entry.entry.metadata_complete && base_metadata_role(entry) == *role
}

fn metadata_sort_role_needs_resort(sort: SortDescriptor) -> bool {
    matches!(sort.role, SortRole::Modified | SortRole::Size)
}

impl PathIndexCache {
    fn prepare(&mut self, generation: u64, len: usize) {
        if self.generation == generation {
            return;
        }

        self.generation = generation;
        self.indexed_until = 0;
        if self.index_by_name.capacity() > len.saturating_mul(2).max(2048) {
            self.index_by_name = std::collections::HashMap::new();
        } else {
            self.index_by_name.clear();
        }
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.indexed_until = 0;
        if self.index_by_name.capacity() > 2048 {
            self.index_by_name = std::collections::HashMap::new();
        } else {
            self.index_by_name.clear();
        }
    }
}

impl ItemIdIndexCache {
    fn prepare(&mut self, generation: u64, len: usize) {
        if self.generation == generation {
            return;
        }

        self.generation = generation;
        self.indexed_until = 0;
        if self.index_by_id.capacity() > len.saturating_mul(2).max(2048) {
            self.index_by_id = std::collections::HashMap::new();
        } else {
            self.index_by_id.clear();
        }
    }

    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.indexed_until = 0;
        if self.index_by_id.capacity() > 2048 {
            self.index_by_id = std::collections::HashMap::new();
        } else {
            self.index_by_id.clear();
        }
    }
}

fn sort_model_entries(entries: &mut [ModelEntry], sort: SortDescriptor) {
    entries.sort_by(|left, right| sort_cmp(left, right, sort));
}

fn merge_sorted_model_entries_in_place(
    entries: &mut Vec<ModelEntry>,
    added: Vec<ModelEntry>,
    sort: SortDescriptor,
) -> ItemRangeList {
    debug_assert!(!added.is_empty());
    debug_assert!(!entries.is_empty());

    let existing_len = entries.len();
    let added_len = added.len();
    let total_len = existing_len + added_len;
    let mut inserted_ranges = Vec::with_capacity(added_len.min(existing_len + 1));

    entries.reserve(added_len);
    let added = ManuallyDrop::new(added);

    unsafe {
        // SAFETY: `entries` has enough spare capacity for `added_len` items.
        // Existing items are moved from the initialized prefix toward the tail
        // with `ptr::read` + `ptr::write`; every vacated source slot is either
        // overwritten by a later write or stays outside the final untouched
        // prefix. Every `added` item is read exactly once before `entries.len`
        // is extended to `total_len`, so both vectors have one owner per item.
        let entries_ptr = entries.as_mut_ptr();
        let added_ptr = added.as_ptr();
        let mut target = total_len;
        let mut existing = existing_len;
        let mut new = added_len;

        while new > 0 {
            let take_existing = existing > 0
                && sort_cmp(
                    &*added_ptr.add(new - 1),
                    &*entries_ptr.add(existing - 1),
                    sort,
                ) == Ordering::Less;

            target -= 1;
            if take_existing {
                existing -= 1;
                ptr::write(
                    entries_ptr.add(target),
                    ptr::read(entries_ptr.add(existing)),
                );
            } else {
                new -= 1;
                record_reverse_inserted_range(&mut inserted_ranges, target);
                ptr::write(entries_ptr.add(target), ptr::read(added_ptr.add(new)));
            }
        }

        entries.set_len(total_len);
    }

    inserted_ranges.reverse();
    inserted_ranges
}

fn record_reverse_inserted_range(ranges: &mut ItemRangeList, index: usize) {
    if let Some(range) = ranges.last_mut()
        && range.start == index + 1
    {
        range.start = index;
        range.len += 1;
        return;
    }
    ranges.push(ItemRange {
        start: index,
        len: 1,
    });
}

fn sort_cmp(left: &ModelEntry, right: &ModelEntry, sort: SortDescriptor) -> Ordering {
    if sort.hidden_last {
        match left_is_hidden(left).cmp(&left_is_hidden(right)) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    if sort.folders_first || sort.role == SortRole::Size {
        match right.is_dir.cmp(&left.is_dir) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    match sort.role {
        SortRole::TrashOriginalPath => trash_original_path_sort_cmp(left, right, sort.order),
        SortRole::TrashDeletionTime => trash_deletion_sort_cmp(left, right, sort.order),
        role => apply_sort_order(role_sort_cmp(left, right, role), sort.order)
            .then_with(|| entry_name_cmp(&left.name, &right.name))
            .then_with(|| {
                left.effective_size_bytes()
                    .cmp(&right.effective_size_bytes())
            }),
    }
}

fn left_is_hidden(entry: &ModelEntry) -> bool {
    entry.name.starts_with('.')
}

fn role_sort_cmp(left: &ModelEntry, right: &ModelEntry, role: SortRole) -> Ordering {
    match role {
        SortRole::Name => entry_name_cmp(&left.name, &right.name),
        SortRole::Modified => left
            .effective_modified_secs()
            .unwrap_or_default()
            .cmp(&right.effective_modified_secs().unwrap_or_default()),
        SortRole::Size => left
            .effective_size_bytes()
            .cmp(&right.effective_size_bytes()),
        SortRole::TrashOriginalPath => Ordering::Equal,
        SortRole::TrashDeletionTime => Ordering::Equal,
    }
}

fn apply_sort_order(ordering: Ordering, order: SortOrder) -> Ordering {
    match order {
        SortOrder::Ascending => ordering,
        SortOrder::Descending => ordering.reverse(),
    }
}

fn trash_deletion_sort_cmp(left: &ModelEntry, right: &ModelEntry, order: SortOrder) -> Ordering {
    trash_sort_bucket(left)
        .cmp(&trash_sort_bucket(right))
        .then_with(|| {
            let left = left.trash_deletion_time.as_deref().unwrap_or_default();
            let right = right.trash_deletion_time.as_deref().unwrap_or_default();
            apply_sort_order(left.cmp(right), order)
        })
        .then_with(|| left.sort_cmp(right))
}

fn trash_original_path_sort_cmp(
    left: &ModelEntry,
    right: &ModelEntry,
    order: SortOrder,
) -> Ordering {
    trash_sort_bucket(left)
        .cmp(&trash_sort_bucket(right))
        .then_with(|| {
            let left = trash_original_path_key(left);
            let right = trash_original_path_key(right);
            apply_sort_order(entry_name_cmp(left.as_ref(), right.as_ref()), order)
        })
        .then_with(|| left.sort_cmp(right))
}

fn trash_original_path_key(entry: &ModelEntry) -> std::borrow::Cow<'_, str> {
    let Some(path) = entry.trash_original_path.as_deref() else {
        return std::borrow::Cow::Borrowed("");
    };
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(path)
        .to_string_lossy()
}

fn trash_sort_bucket(entry: &ModelEntry) -> u8 {
    if entry.trash_deletion_time.is_some() {
        0
    } else if entry.trash_original_path.is_some() {
        1
    } else {
        2
    }
}

fn range_all(len: usize) -> ItemRangeList {
    if len == 0 {
        Vec::new()
    } else {
        vec![ItemRange { start: 0, len }]
    }
}

fn ranges_from_indexes(mut indexes: Vec<usize>) -> ItemRangeList {
    indexes.sort_unstable();
    indexes.dedup();
    let mut ranges: ItemRangeList = Vec::new();
    for index in indexes {
        if let Some(last) = ranges.last_mut()
            && last.start + last.len == index
        {
            last.len += 1;
            continue;
        }
        ranges.push(ItemRange {
            start: index,
            len: 1,
        });
    }
    ranges
}

