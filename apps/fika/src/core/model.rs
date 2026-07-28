use super::entries::{
    Entry, EntryMetadataRole, ItemId, ModelEntry, directory_entry_path, entry_name_cmp,
};
use super::file_ops;
use super::mime::mime_magic_resolution_required;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRange {
    pub start: usize,
    pub len: usize,
}

pub type ItemRangeList = Vec<ItemRange>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangedRoles {
    pub metadata: bool,
    pub name: bool,
    pub path: bool,
}

impl ChangedRoles {
    pub const ALL: Self = Self {
        metadata: true,
        name: true,
        path: true,
    };

    pub fn metadata() -> Self {
        Self {
            metadata: true,
            name: false,
            path: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryModelSignal {
    ItemsInserted(ItemRangeList),
    ItemsRemoved(ItemRangeList),
    ItemsChanged(ItemRangeList, ChangedRoles),
    ItemsMoved(Vec<(usize, usize)>),
    GroupsChanged,
    SortChanged,
    ModelReset,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SortRole {
    Name,
    Modified,
    Size,
    TrashOriginalPath,
    TrashDeletionTime,
}

impl SortRole {
    pub fn default_order(self) -> SortOrder {
        match self {
            Self::Name | Self::TrashOriginalPath => SortOrder::Ascending,
            Self::Modified | Self::Size | Self::TrashDeletionTime => SortOrder::Descending,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortDescriptor {
    pub role: SortRole,
    pub order: SortOrder,
    pub folders_first: bool,
    pub hidden_last: bool,
}

impl SortDescriptor {
    pub fn for_directory(directory: &Path) -> Self {
        if file_ops::is_trash_files_dir(directory) {
            Self {
                role: SortRole::TrashDeletionTime,
                order: SortOrder::Descending,
                folders_first: true,
                hidden_last: false,
            }
        } else {
            Self::default()
        }
    }
}

impl Default for SortDescriptor {
    fn default() -> Self {
        Self {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: false,
        }
    }
}

#[derive(Debug)]
pub struct DirectoryModel {
    data: DirectoryModelData,
    next_item_id: u64,
    data_generation: u64,
    index_generation: u64,
    path_index: RefCell<PathIndexCache>,
    id_index: RefCell<ItemIdIndexCache>,
}

#[derive(Clone, Debug, Default)]
struct DirectoryModelData {
    directory: PathBuf,
    entries: Vec<ModelEntry>,
    sort: SortDescriptor,
}

#[derive(Clone, Debug, Default)]
struct PathIndexCache {
    generation: u64,
    indexed_until: usize,
    index_by_name: HashMap<Arc<str>, usize>,
}

#[derive(Clone, Debug, Default)]
struct ItemIdIndexCache {
    generation: u64,
    indexed_until: usize,
    index_by_id: HashMap<ItemId, usize>,
}

impl Default for DirectoryModel {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryModel {
    pub fn new() -> Self {
        Self::for_directory(PathBuf::new())
    }

    pub fn for_directory(directory: PathBuf) -> Self {
        let sort = SortDescriptor::for_directory(&directory);
        Self {
            data: DirectoryModelData {
                directory,
                entries: Vec::new(),
                sort,
            },
            next_item_id: 0,
            data_generation: 0,
            index_generation: 0,
            path_index: RefCell::new(PathIndexCache::default()),
            id_index: RefCell::new(ItemIdIndexCache::default()),
        }
    }

    pub fn fork_for_pane(&self) -> Self {
        Self {
            data: self.data.clone(),
            next_item_id: self.next_item_id,
            data_generation: self.data_generation,
            index_generation: self.index_generation,
            path_index: RefCell::new(PathIndexCache::default()),
            id_index: RefCell::new(ItemIdIndexCache::default()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.data.directory
    }

    pub fn entries(&self) -> &[ModelEntry] {
        &self.data.entries
    }

    pub fn listing_snapshot(&self) -> Arc<Vec<Entry>> {
        Arc::new(
            self.data
                .entries
                .iter()
                .map(|entry| entry.entry.clone())
                .collect(),
        )
    }

    pub fn sort_descriptor(&self) -> SortDescriptor {
        self.data.sort
    }

    pub fn len(&self) -> usize {
        self.data.entries.len()
    }

    pub fn data_generation(&self) -> u64 {
        self.data_generation
    }

    pub fn structure_generation(&self) -> u64 {
        self.index_generation
    }

    pub fn is_empty(&self) -> bool {
        self.data.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&ModelEntry> {
        self.data.entries.get(index)
    }

    pub fn path_for_index(&self, index: usize) -> Option<PathBuf> {
        self.data
            .entries
            .get(index)
            .map(|entry| self.path_for_entry(entry))
    }

    pub fn path_for_entry(&self, entry: &ModelEntry) -> PathBuf {
        if let Some(target_path) = &entry.target_path {
            return target_path.clone();
        }
        self.data.directory.join(entry.name.as_ref())
    }

    pub fn index_of_path(&self, path: &Path) -> Option<usize> {
        if let Some(index) = self
            .data
            .entries
            .iter()
            .position(|entry| entry.target_path.as_deref() == Some(path))
        {
            return Some(index);
        }
        let item_path = directory_entry_path(&self.data.directory, path)?;
        let name = item_path.file_name()?.to_string_lossy();
        self.index_of_name(name.as_ref())
    }

    pub fn index_of_id(&self, id: ItemId) -> Option<usize> {
        if !id.is_assigned() {
            return None;
        }
        self.index_of_item_id(id)
    }

    pub fn set_thumbnail_path(
        &mut self,
        id: ItemId,
        thumbnail_path: Option<PathBuf>,
    ) -> Vec<DirectoryModelSignal> {
        let Some(index) = self.index_of_id(id) else {
            return Vec::new();
        };
        if self.data.entries[index].thumbnail_path == thumbnail_path
            && !self.data.entries[index].thumbnail_failed
        {
            return Vec::new();
        }

        self.data.entries[index].thumbnail_path = thumbnail_path;
        self.data.entries[index].thumbnail_failed = false;
        self.mark_metadata_changed();
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange {
                start: index,
                len: 1,
            }],
            ChangedRoles::metadata(),
        )]
    }

    pub fn set_thumbnail_failed(&mut self, id: ItemId, failed: bool) -> Vec<DirectoryModelSignal> {
        let Some(index) = self.index_of_id(id) else {
            return Vec::new();
        };
        if self.data.entries[index].thumbnail_failed == failed
            && (failed || self.data.entries[index].thumbnail_path.is_none())
        {
            return Vec::new();
        }

        self.data.entries[index].thumbnail_failed = failed;
        if failed {
            self.data.entries[index].thumbnail_path = None;
        }
        self.mark_metadata_changed();
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange {
                start: index,
                len: 1,
            }],
            ChangedRoles::metadata(),
        )]
    }

    pub fn set_metadata_role(
        &mut self,
        id: ItemId,
        path: &Path,
        role: EntryMetadataRole,
    ) -> Vec<DirectoryModelSignal> {
        let Some(index) = self.index_of_id(id) else {
            return Vec::new();
        };
        if self.path_for_index(index).as_deref() != Some(path) {
            return Vec::new();
        }

        if self.data.entries[index].effective_metadata_role() == role {
            let normalized_role =
                (!metadata_role_matches_base_entry(&self.data.entries[index], &role))
                    .then_some(role);
            if self.data.entries[index].metadata_refresh_pending
                || self.data.entries[index].metadata_role != normalized_role
            {
                self.data.entries[index].metadata_role = normalized_role;
                self.data.entries[index].metadata_refresh_pending = false;
                self.mark_metadata_changed();
            }
            return Vec::new();
        }

        let old_order = metadata_sort_role_needs_resort(self.data.sort).then(|| {
            self.data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>()
        });
        let had_reusable_thumbnail = self.data.entries[index].thumbnail_path.is_some();
        let old_size = self.data.entries[index].effective_size_bytes();
        let old_modified = self.data.entries[index].effective_modified_secs();
        let old_mime = self.data.entries[index].effective_mime_type_cloned();
        self.data.entries[index].metadata_role =
            (!metadata_role_matches_base_entry(&self.data.entries[index], &role))
                .then_some(role.clone());
        self.data.entries[index].metadata_refresh_pending = false;
        if had_reusable_thumbnail
            && (old_size != self.data.entries[index].effective_size_bytes()
                || old_modified != self.data.entries[index].effective_modified_secs())
        {
            self.data.entries[index].thumbnail_path = None;
        }
        if old_size != self.data.entries[index].effective_size_bytes()
            || old_modified != self.data.entries[index].effective_modified_secs()
            || old_mime != self.data.entries[index].effective_mime_type_cloned()
        {
            self.data.entries[index].thumbnail_failed = false;
        }

        if let Some(old_order) = old_order {
            let sort = self.data.sort;
            sort_model_entries(&mut self.data.entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if order_changed {
                self.mark_structure_changed();
                return vec![DirectoryModelSignal::ModelReset];
            }
        }

        let index = self.index_of_id(id).unwrap_or(index);
        self.mark_metadata_changed();
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange {
                start: index,
                len: 1,
            }],
            ChangedRoles::metadata(),
        )]
    }

    pub fn clear_for_directory(&mut self, directory: PathBuf) -> Vec<DirectoryModelSignal> {
        if self.data.directory == directory && self.data.entries.is_empty() {
            return Vec::new();
        }
        self.replace_data(directory, Vec::new());
        vec![DirectoryModelSignal::ModelReset]
    }

    pub fn replace_listing(
        &mut self,
        directory: PathBuf,
        entries: Arc<Vec<Entry>>,
    ) -> Vec<DirectoryModelSignal> {
        let mut entries = entries
            .iter()
            .cloned()
            .map(ModelEntry::unassigned)
            .collect::<Vec<_>>();
        let sort = if self.data.directory == directory {
            self.data.sort
        } else {
            SortDescriptor::for_directory(&directory)
        };
        self.assign_listing_identity(&directory, &mut entries);
        sort_model_entries(&mut entries, sort);
        if self.same_listing(&directory, &entries) {
            self.replace_same_listing_metadata(entries);
            return vec![DirectoryModelSignal::ItemsChanged(
                range_all(self.data.entries.len()),
                ChangedRoles::metadata(),
            )];
        }
        self.replace_data(directory, entries);
        vec![DirectoryModelSignal::ModelReset]
    }

    pub fn apply_items_added(&mut self, entries: Vec<Entry>) -> Vec<DirectoryModelSignal> {
        if entries.is_empty() {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let mut changed_structure = false;
        let mut added = Vec::new();
        for entry in entries {
            if let Some(index) = self.index_of_entry_name(entry.name.as_ref()) {
                let mut entry = ModelEntry::unassigned(entry);
                let old = self.data.entries[index].clone();
                self.assign_identity_from_index(&mut entry, index);
                changed_structure |= old.is_dir != entry.is_dir;
                self.data_mut().entries[index] = entry;
                changed.push(index);
            } else {
                let mut entry = ModelEntry::unassigned(entry);
                self.assign_new_identity(&mut entry);
                added.push(entry);
            }
        }
        if !added.is_empty() {
            let sort = self.data.sort;
            if !changed.is_empty() {
                let data = self.data_mut();
                data.entries.extend(added);
                sort_model_entries(&mut data.entries, sort);
                self.mark_structure_changed();
                return vec![DirectoryModelSignal::ModelReset];
            }
            sort_model_entries(&mut added, sort);
            let item_ranges = self.insert_sorted_model_entries(added);
            self.mark_structure_changed();
            return vec![DirectoryModelSignal::ItemsInserted(item_ranges)];
        }

        if !changed.is_empty() {
            let sort = self.data.sort;
            let old_order = self
                .data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            sort_model_entries(&mut self.data_mut().entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if changed_structure || order_changed {
                self.mark_structure_changed();
                vec![DirectoryModelSignal::ModelReset]
            } else {
                self.mark_metadata_changed();
                vec![DirectoryModelSignal::ItemsChanged(
                    ranges_from_indexes(changed),
                    ChangedRoles::ALL,
                )]
            }
        } else {
            Vec::new()
        }
    }

    fn insert_sorted_model_entries(&mut self, added: Vec<ModelEntry>) -> ItemRangeList {
        if added.is_empty() {
            return Vec::new();
        }

        if self.data.entries.is_empty() {
            let len = added.len();
            self.data.entries = added;
            return vec![ItemRange { start: 0, len }];
        }

        let sort = self.data.sort;
        merge_sorted_model_entries_in_place(&mut self.data.entries, added, sort)
    }

    pub fn apply_items_deleted(&mut self, paths: &[PathBuf]) -> Vec<DirectoryModelSignal> {
        if paths.is_empty() {
            return Vec::new();
        }
        let removed_indexes = paths
            .iter()
            .filter_map(|path| self.index_of_path(path))
            .collect::<Vec<_>>();
        if removed_indexes.is_empty() {
            return Vec::new();
        }
        let deleted = removed_indexes.iter().copied().collect::<BTreeSet<_>>();
        let mut index = 0usize;
        self.data_mut().entries.retain(|_| {
            let keep = !deleted.contains(&index);
            index += 1;
            keep
        });
        self.mark_structure_changed();
        vec![DirectoryModelSignal::ItemsRemoved(ranges_from_indexes(
            removed_indexes,
        ))]
    }

    pub fn apply_items_refreshed(
        &mut self,
        pairs: Vec<super::directory::RefreshPair>,
    ) -> Vec<DirectoryModelSignal> {
        if pairs.is_empty() {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let mut changed_structure = false;
        let mut removed = Vec::new();
        let mut added = Vec::new();

        for pair in pairs {
            match pair.entry {
                Some(entry) => {
                    if let Some(index) = self.index_of_path(&pair.old_path) {
                        let mut entry = ModelEntry::unassigned(entry);
                        let old = self.data.entries[index].clone();
                        self.assign_identity_from_index(&mut entry, index);
                        changed_structure |= old.name != entry.name || old.is_dir != entry.is_dir;
                        self.data_mut().entries[index] = entry;
                        changed.push(index);
                    } else {
                        added.push(entry);
                    }
                }
                None => {
                    removed.push(pair.old_path);
                }
            }
        }

        let mut signals = Vec::new();
        if !removed.is_empty() {
            signals.extend(self.apply_items_deleted(&removed));
        }
        if !added.is_empty() {
            signals.extend(self.apply_items_added(added));
        }
        if !changed.is_empty() {
            let sort = self.data.sort;
            let old_order = self
                .data
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            sort_model_entries(&mut self.data_mut().entries, sort);
            let order_changed = old_order
                .iter()
                .zip(&self.data.entries)
                .any(|(old_id, entry)| *old_id != entry.id);
            if changed_structure || order_changed {
                self.mark_structure_changed();
                signals.push(DirectoryModelSignal::ModelReset);
            } else {
                self.mark_metadata_changed();
                signals.push(DirectoryModelSignal::ItemsChanged(
                    ranges_from_indexes(changed),
                    ChangedRoles::ALL,
                ));
            }
        }
        signals
    }

    pub fn set_sort(&mut self, sort: SortDescriptor) -> Vec<DirectoryModelSignal> {
        if self.data.sort == sort {
            return Vec::new();
        }
        self.data.sort = sort;
        sort_model_entries(&mut self.data.entries, sort);
        self.mark_structure_changed();
        vec![DirectoryModelSignal::SortChanged]
    }

    fn same_listing(&self, directory: &Path, entries: &[ModelEntry]) -> bool {
        self.data.directory == directory
            && self.data.entries.len() == entries.len()
            && self
                .data
                .entries
                .iter()
                .zip(entries)
                .all(|(left, right)| left.name == right.name && left.is_dir == right.is_dir)
    }

    fn assign_listing_identity(&mut self, directory: &Path, entries: &mut [ModelEntry]) {
        if self.data.directory != directory {
            for entry in entries {
                self.assign_new_identity(entry);
            }
            return;
        }

        self.assign_listing_identity_by_name(entries);
    }

    fn assign_listing_identity_by_name(&mut self, entries: &mut [ModelEntry]) {
        let mut old_indexes = (0..self.data.entries.len()).collect::<Vec<_>>();
        old_indexes.sort_by(|left, right| {
            entry_name_cmp(
                &self.data.entries[*left].name,
                &self.data.entries[*right].name,
            )
        });
        let mut new_indexes = (0..entries.len()).collect::<Vec<_>>();
        new_indexes
            .sort_by(|left, right| entry_name_cmp(&entries[*left].name, &entries[*right].name));

        let mut old_cursor = 0usize;
        for new_index in new_indexes {
            if entries[new_index].id.is_assigned() {
                self.next_item_id = self.next_item_id.max(entries[new_index].id.0);
                continue;
            }

            let mut reused_index = None;
            while let Some(old_index) = old_indexes.get(old_cursor).copied() {
                match entry_name_cmp(&self.data.entries[old_index].name, &entries[new_index].name) {
                    std::cmp::Ordering::Less => old_cursor += 1,
                    std::cmp::Ordering::Equal => {
                        reused_index = Some(old_index);
                        old_cursor += 1;
                        break;
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }
            if let Some(old_index) = reused_index {
                entries[new_index].id = self.data.entries[old_index].id;
                preserve_refreshed_entry_roles(
                    &self.data.entries[old_index],
                    &mut entries[new_index],
                );
            } else {
                entries[new_index].id = self.allocate_item_id();
            }
        }
    }

    fn assign_identity_from_index(&mut self, entry: &mut ModelEntry, index: usize) {
        if entry.id.is_assigned() {
            self.next_item_id = self.next_item_id.max(entry.id.0);
        } else {
            entry.id = self
                .data
                .entries
                .get(index)
                .map(|entry| entry.id)
                .unwrap_or_else(|| self.allocate_item_id());
        }
        if let Some(old) = self.data.entries.get(index) {
            preserve_refreshed_entry_roles(old, entry);
        }
    }

    fn assign_new_identity(&mut self, entry: &mut ModelEntry) {
        if entry.id.is_assigned() {
            self.next_item_id = self.next_item_id.max(entry.id.0);
        } else {
            entry.id = self.allocate_item_id();
        }
    }

    fn replace_data(&mut self, directory: PathBuf, entries: Vec<ModelEntry>) {
        let sort = if self.data.directory == directory {
            self.data.sort
        } else {
            SortDescriptor::for_directory(&directory)
        };
        self.data = DirectoryModelData {
            directory,
            entries,
            sort,
        };
        self.data_generation = self.data_generation.wrapping_add(1);
        self.index_generation = self.index_generation.wrapping_add(1);
        self.reset_indexes();
    }

    fn replace_same_listing_metadata(&mut self, mut entries: Vec<ModelEntry>) {
        for (old, new) in self.data.entries.iter().zip(&mut entries) {
            let mut data = (*new.entry).clone();
            data.name = Arc::clone(&old.name);
            data.name_width_units = old.name_width_units;
            new.entry = Entry::new(data);
            new.id = old.id;
            preserve_refreshed_entry_roles(old, new);
        }
        self.data.entries = entries;
        self.mark_metadata_changed();
    }

    fn data_mut(&mut self) -> &mut DirectoryModelData {
        &mut self.data
    }

    fn mark_metadata_changed(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
    }

    fn mark_structure_changed(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.index_generation = self.index_generation.wrapping_add(1);
        self.reset_indexes();
    }

    fn index_of_entry_name(&self, name: &str) -> Option<usize> {
        self.index_of_name(name)
    }

    fn index_of_name(&self, name: &str) -> Option<usize> {
        const INDEX_BLOCK_SIZE: usize = 1000;

        let mut cache = self.path_index.borrow_mut();
        cache.prepare(self.index_generation, self.data.entries.len());

        loop {
            if let Some(index) = cache.index_by_name.get(name).copied() {
                return Some(index);
            }

            if cache.indexed_until >= self.data.entries.len() {
                return None;
            }

            let end = (cache.indexed_until + INDEX_BLOCK_SIZE).min(self.data.entries.len());
            for index in cache.indexed_until..end {
                cache
                    .index_by_name
                    .insert(Arc::clone(&self.data.entries[index].name), index);
            }
            cache.indexed_until = end;
        }
    }

    fn index_of_item_id(&self, id: ItemId) -> Option<usize> {
        const INDEX_BLOCK_SIZE: usize = 1000;

        let mut cache = self.id_index.borrow_mut();
        cache.prepare(self.index_generation, self.data.entries.len());

        loop {
            if let Some(index) = cache.index_by_id.get(&id).copied() {
                return Some(index);
            }

            if cache.indexed_until >= self.data.entries.len() {
                return None;
            }

            let end = (cache.indexed_until + INDEX_BLOCK_SIZE).min(self.data.entries.len());
            for index in cache.indexed_until..end {
                let item_id = self.data.entries[index].id;
                if item_id.is_assigned() {
                    cache.index_by_id.insert(item_id, index);
                }
            }
            cache.indexed_until = end;
        }
    }

    fn reset_indexes(&self) {
        self.path_index.borrow_mut().reset();
        self.id_index.borrow_mut().reset();
    }

    fn allocate_item_id(&mut self) -> ItemId {
        self.next_item_id += 1;
        ItemId(self.next_item_id)
    }
}

include!("model/sort_and_roles.rs");

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
