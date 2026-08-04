use super::directory::RefreshPair;
use super::entries::{Entry, directory_entry_path, entry_sort_cmp, sort_entries};
use super::file_ops;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::mem::ManuallyDrop;
use std::path::{Component, Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Debug)]
pub struct DirectoryCacheSnapshot {
    path: PathBuf,
    entries: Arc<Vec<Entry>>,
    loaded_at: u64,
    fingerprint: Option<DirectoryCacheFingerprint>,
}

impl DirectoryCacheSnapshot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &Arc<Vec<Entry>> {
        &self.entries
    }

    pub fn loaded_at(&self) -> u64 {
        self.loaded_at
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    fn matches_current_directory(&self) -> bool {
        self.fingerprint
            .is_none_or(|fingerprint| fingerprint.matches_path(&self.path))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryCacheStats {
    pub hits: usize,
    pub misses: usize,
    pub stale_invalidations: usize,
    pub evicted_directories: usize,
    pub skipped_large_directories: usize,
    pub cached_entries: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryCacheDirectorySummary {
    path: PathBuf,
    entry_count: usize,
    observed_at: u64,
}

impl DirectoryCacheDirectorySummary {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub fn observed_at(&self) -> u64 {
        self.observed_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryCacheDebugSnapshot {
    stats: DirectoryCacheStats,
    limits: DirectoryCacheLimits,
    cached_directories: Vec<DirectoryCacheDirectorySummary>,
    skipped_large_directories: Vec<DirectoryCacheDirectorySummary>,
}

impl DirectoryCacheDebugSnapshot {
    pub fn stats(&self) -> DirectoryCacheStats {
        self.stats
    }

    pub fn limits(&self) -> DirectoryCacheLimits {
        self.limits
    }

    pub fn cached_directories(&self) -> &[DirectoryCacheDirectorySummary] {
        &self.cached_directories
    }

    pub fn skipped_large_directories(&self) -> &[DirectoryCacheDirectorySummary] {
        &self.skipped_large_directories
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectoryCacheFingerprint {
    modified_nanos: Option<u128>,
    len: u64,
    is_dir: bool,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryCacheLimits {
    pub max_dirs: usize,
    pub max_entries: usize,
    pub max_entries_per_dir: usize,
}

impl Default for DirectoryCacheLimits {
    fn default() -> Self {
        Self {
            max_dirs: 32,
            max_entries: 50_000,
            max_entries_per_dir: 10_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryCache {
    limits: DirectoryCacheLimits,
    clock: u64,
    stats: DirectoryCacheStats,
    cached_entries: usize,
    entries_by_path: HashMap<PathBuf, DirectoryCacheSnapshot>,
    lru: VecDeque<PathBuf>,
    skipped_large_by_path: HashMap<PathBuf, DirectoryCacheDirectorySummary>,
    skipped_large_lru: VecDeque<PathBuf>,
}

impl Default for DirectoryCache {
    fn default() -> Self {
        Self::with_limits(DirectoryCacheLimits::default())
    }
}

impl DirectoryCache {
    pub fn new(max_dirs: usize) -> Self {
        Self::with_limits(DirectoryCacheLimits {
            max_dirs,
            ..DirectoryCacheLimits::default()
        })
    }

    pub fn with_limits(limits: DirectoryCacheLimits) -> Self {
        Self {
            limits: DirectoryCacheLimits {
                max_dirs: limits.max_dirs.max(1),
                max_entries: limits.max_entries.max(1),
                max_entries_per_dir: limits.max_entries_per_dir.max(1),
            },
            clock: 0,
            stats: DirectoryCacheStats::default(),
            cached_entries: 0,
            entries_by_path: HashMap::new(),
            lru: VecDeque::new(),
            skipped_large_by_path: HashMap::new(),
            skipped_large_lru: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries_by_path.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries_by_path.is_empty()
    }

    pub fn cached_entry_count(&self) -> usize {
        self.cached_entries
    }

    pub fn can_store_entry_count(&self, entry_count: usize) -> bool {
        entry_count <= self.limits.max_entries_per_dir && entry_count <= self.limits.max_entries
    }

    pub fn stats(&self) -> DirectoryCacheStats {
        DirectoryCacheStats {
            cached_entries: self.cached_entries,
            ..self.stats
        }
    }

    pub fn debug_snapshot(&self) -> DirectoryCacheDebugSnapshot {
        let cached_directories = self
            .lru
            .iter()
            .filter_map(|path| self.entries_by_path.get(path))
            .map(|snapshot| DirectoryCacheDirectorySummary {
                path: snapshot.path.clone(),
                entry_count: snapshot.entry_count(),
                observed_at: snapshot.loaded_at,
            })
            .collect();
        let skipped_large_directories = self
            .skipped_large_lru
            .iter()
            .filter_map(|path| self.skipped_large_by_path.get(path).cloned())
            .collect();

        DirectoryCacheDebugSnapshot {
            stats: self.stats(),
            limits: self.limits,
            cached_directories,
            skipped_large_directories,
        }
    }

    pub fn get(&mut self, path: &Path) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path);
        let snapshot = self.entries_by_path.get(&key).cloned();
        if snapshot.is_some() {
            self.stats.hits += 1;
            self.touch(&key);
        } else {
            self.stats.misses += 1;
        }
        snapshot
    }

    pub fn get_fresh(&mut self, path: &Path) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path);
        let snapshot = self.get(&key)?;
        if !snapshot.matches_current_directory() {
            self.remove_normalized(&key);
            self.stats.stale_invalidations += 1;
            return None;
        }
        Some(snapshot)
    }

    pub fn insert_fresh(
        &mut self,
        path: impl AsRef<Path>,
        entries: Arc<Vec<Entry>>,
    ) -> Option<DirectoryCacheSnapshot> {
        let key = normalize_cache_path(path.as_ref());
        if !self.can_store_entry_count(entries.len()) {
            self.remove_normalized(&key);
            self.record_skipped_large_normalized(key, entries.len());
            self.stats.skipped_large_directories += 1;
            return None;
        }

        self.clock = self.clock.wrapping_add(1);
        let snapshot = DirectoryCacheSnapshot {
            path: key.clone(),
            entries,
            loaded_at: self.clock,
            fingerprint: DirectoryCacheFingerprint::for_path(&key),
        };
        self.remove_normalized(&key);
        self.remove_skipped_large_normalized(&key);
        self.cached_entries += snapshot.entry_count();
        self.entries_by_path.insert(key.clone(), snapshot.clone());
        self.touch(&key);
        self.evict_oldest();
        Some(snapshot)
    }

    pub fn mark_stale(&mut self, path: &Path) -> bool {
        let key = normalize_cache_path(path);
        let removed = self.remove_normalized(&key);
        if removed {
            self.stats.stale_invalidations += 1;
        }
        removed
    }

    pub fn record_uncached_directory(&mut self, path: &Path, entry_count: usize) -> bool {
        if self.can_store_entry_count(entry_count) {
            return false;
        }
        let key = normalize_cache_path(path);
        self.remove_normalized(&key);
        self.record_skipped_large_normalized(key, entry_count);
        self.stats.skipped_large_directories += 1;
        true
    }

    pub fn apply_items_added(&mut self, path: &Path, entries: &[Entry]) -> bool {
        if entries.is_empty() {
            return false;
        }
        self.update_fresh_entries(path, |directory, cached_entries| {
            let mut replaced_existing = false;
            let mut index_by_name = HashMap::with_capacity(cached_entries.len());
            for (index, entry) in cached_entries.iter().enumerate() {
                index_by_name.insert(Arc::clone(&entry.name), index);
            }

            let mut added = Vec::new();
            let mut added_index_by_name = HashMap::new();
            for entry in entries {
                if let Some(index) = index_by_name.get(entry.name.as_ref()).copied() {
                    cached_entries[index] = entry.clone();
                    replaced_existing = true;
                } else if let Some(index) = added_index_by_name.get(entry.name.as_ref()).copied() {
                    added[index] = entry.clone();
                } else {
                    added_index_by_name.insert(Arc::clone(&entry.name), added.len());
                    added.push(entry.clone());
                }
            }

            if replaced_existing {
                sort_cache_entries(directory, cached_entries);
            }
            if !added.is_empty() {
                sort_cache_entries(directory, &mut added);
                merge_sorted_cache_entries(
                    cached_entries,
                    added,
                    file_ops::is_trash_files_dir(directory),
                );
            }
            true
        })
    }

    pub fn apply_items_deleted(&mut self, path: &Path, paths: &[PathBuf]) -> bool {
        self.update_fresh_entries(path, |directory, cached_entries| {
            let removed_indexes = paths
                .iter()
                .filter_map(|path| entry_index_for_path(cached_entries, directory, path))
                .collect::<BTreeSet<_>>();
            if removed_indexes.is_empty() {
                return true;
            }
            let mut index = 0usize;
            cached_entries.retain(|_| {
                let keep = !removed_indexes.contains(&index);
                index += 1;
                keep
            });
            true
        })
    }

    pub fn apply_items_refreshed(&mut self, path: &Path, pairs: &[RefreshPair]) -> bool {
        self.update_fresh_entries(path, |directory, cached_entries| {
            for pair in pairs {
                match &pair.entry {
                    Some(entry) => {
                        if let Some(index) =
                            entry_index_for_path(cached_entries, directory, &pair.old_path).or_else(
                                || entry_index_for_name(cached_entries, entry.name.as_ref()),
                            )
                        {
                            cached_entries[index] = entry.clone();
                        } else {
                            cached_entries.push(entry.clone());
                        }
                    }
                    None => {
                        let Some(index) =
                            entry_index_for_path(cached_entries, directory, &pair.old_path)
                        else {
                            continue;
                        };
                        cached_entries.remove(index);
                    }
                }
            }
            sort_cache_entries(directory, cached_entries);
            true
        })
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let key = normalize_cache_path(path);
        self.remove_normalized(&key) | self.remove_skipped_large_normalized(&key)
    }

    fn update_fresh_entries(
        &mut self,
        path: &Path,
        update: impl FnOnce(&Path, &mut Vec<Entry>) -> bool,
    ) -> bool {
        let key = normalize_cache_path(path);
        let Some(snapshot) = self.entries_by_path.get(&key).cloned() else {
            return false;
        };

        let mut entries = snapshot.entries.iter().cloned().collect::<Vec<_>>();
        if !update(&key, &mut entries) {
            return false;
        }
        self.replace_fresh_entries(key, entries, snapshot.entry_count())
    }

    fn replace_fresh_entries(
        &mut self,
        key: PathBuf,
        entries: Vec<Entry>,
        old_entry_count: usize,
    ) -> bool {
        if !self.can_store_entry_count(entries.len()) {
            self.remove_normalized(&key);
            self.record_skipped_large_normalized(key, entries.len());
            self.stats.skipped_large_directories += 1;
            return false;
        }

        self.clock = self.clock.wrapping_add(1);
        let new_entry_count = entries.len();
        let snapshot = DirectoryCacheSnapshot {
            path: key.clone(),
            entries: Arc::new(entries),
            loaded_at: self.clock,
            fingerprint: DirectoryCacheFingerprint::for_path(&key),
        };
        self.cached_entries = self
            .cached_entries
            .saturating_sub(old_entry_count)
            .saturating_add(new_entry_count);
        self.entries_by_path.insert(key.clone(), snapshot);
        self.remove_skipped_large_normalized(&key);
        self.touch(&key);
        self.evict_oldest();
        true
    }

    fn remove_normalized(&mut self, key: &Path) -> bool {
        self.lru.retain(|candidate| candidate.as_path() != key);
        let Some(removed) = self.entries_by_path.remove(key) else {
            return false;
        };
        self.cached_entries = self.cached_entries.saturating_sub(removed.entry_count());
        true
    }

    fn remove_skipped_large_normalized(&mut self, key: &Path) -> bool {
        self.skipped_large_lru
            .retain(|candidate| candidate.as_path() != key);
        self.skipped_large_by_path.remove(key).is_some()
    }

    fn record_skipped_large_normalized(&mut self, key: PathBuf, entry_count: usize) {
        self.clock = self.clock.wrapping_add(1);
        self.skipped_large_lru
            .retain(|candidate| candidate.as_path() != key);
        let summary = DirectoryCacheDirectorySummary {
            path: key.clone(),
            entry_count,
            observed_at: self.clock,
        };
        self.skipped_large_by_path.insert(key.clone(), summary);
        self.skipped_large_lru.push_back(key);
        while self.skipped_large_lru.len() > self.limits.max_dirs {
            let Some(path) = self.skipped_large_lru.pop_front() else {
                break;
            };
            self.skipped_large_by_path.remove(&path);
        }
    }

    fn touch(&mut self, key: &Path) {
        self.lru.retain(|candidate| candidate.as_path() != key);
        self.lru.push_back(key.to_path_buf());
    }

    fn evict_oldest(&mut self) {
        while self.entries_by_path.len() > self.limits.max_dirs
            || self.cached_entries > self.limits.max_entries
        {
            let Some(path) = self.lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries_by_path.remove(&path) {
                self.cached_entries = self.cached_entries.saturating_sub(removed.entry_count());
                self.stats.evicted_directories += 1;
            }
        }
    }
}

impl DirectoryCacheFingerprint {
    fn for_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified_nanos: metadata.modified().ok().and_then(system_time_nanos),
            len: metadata.len(),
            is_dir: metadata.is_dir(),
            #[cfg(unix)]
            dev: metadata.dev(),
            #[cfg(unix)]
            ino: metadata.ino(),
        })
    }

    fn matches_path(self, path: &Path) -> bool {
        Self::for_path(path) == Some(self)
    }
}

fn system_time_nanos(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

fn entry_index_for_path(entries: &[Entry], directory: &Path, path: &Path) -> Option<usize> {
    let item_path = directory_entry_path(directory, path)?;
    let name = item_path.file_name()?.to_string_lossy();
    let name = name.trim();
    entry_index_for_name(entries, name)
}

fn entry_index_for_name(entries: &[Entry], name: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.name.as_ref() == name)
}

fn sort_cache_entries(directory: &Path, entries: &mut [Entry]) {
    sort_entries(entries, file_ops::is_trash_files_dir(directory));
}

fn merge_sorted_cache_entries(entries: &mut Vec<Entry>, added: Vec<Entry>, trash: bool) {
    if added.is_empty() {
        return;
    }
    if entries.is_empty() {
        *entries = added;
        return;
    }

    let existing_len = entries.len();
    let added_len = added.len();
    let total_len = existing_len + added_len;

    entries.reserve(added_len);
    let added = ManuallyDrop::new(added);

    unsafe {
        // SAFETY: `entries` has capacity for the final length. Existing cache
        // entries are moved once from the initialized prefix toward the tail,
        // and every item from `added` is read exactly once before the final
        // length is published.
        let entries_ptr = entries.as_mut_ptr();
        let added_ptr = added.as_ptr();
        let mut target = total_len;
        let mut existing = existing_len;
        let mut new = added_len;

        while new > 0 {
            let take_existing = existing > 0
                && entry_sort_cmp(
                    &*added_ptr.add(new - 1),
                    &*entries_ptr.add(existing - 1),
                    trash,
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
                ptr::write(entries_ptr.add(target), ptr::read(added_ptr.add(new)));
            }
        }

        entries.set_len(total_len);
    }
}

pub fn normalize_cache_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests;
