use super::*;
use crate::core::entries::EntryData;
use std::process;
use std::time::Duration;

fn entry(name: &str) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: false,
    })
}

fn entries(names: &[&str]) -> Arc<Vec<Entry>> {
    Arc::new(names.iter().map(|name| entry(name)).collect())
}

fn entry_names(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.name.to_string()).collect()
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tensor-files-cache-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn cache_normalizes_paths_for_lookup() {
    let mut cache = DirectoryCache::new(4);
    let payload = entries(&["a"]);

    assert!(
        cache
            .insert_fresh("/tmp/tensor-files/../tensor-files", Arc::clone(&payload))
            .is_some()
    );
    let snapshot = cache.get(Path::new("/tmp/tensor-files")).unwrap();

    assert!(Arc::ptr_eq(snapshot.entries(), &payload));
    assert_eq!(snapshot.path(), Path::new("/tmp/tensor-files"));
    assert_eq!(cache.stats().hits, 1);
    assert_eq!(cache.stats().misses, 0);
}

#[test]
fn cache_evicts_least_recently_used_directory() {
    let mut cache = DirectoryCache::new(2);

    assert!(cache.insert_fresh("/tmp/a", entries(&["a"])).is_some());
    assert!(cache.insert_fresh("/tmp/b", entries(&["b"])).is_some());
    assert!(cache.get(Path::new("/tmp/a")).is_some());
    assert!(cache.insert_fresh("/tmp/c", entries(&["c"])).is_some());

    assert!(cache.get(Path::new("/tmp/a")).is_some());
    assert!(cache.get(Path::new("/tmp/b")).is_none());
    assert!(cache.get(Path::new("/tmp/c")).is_some());
    assert_eq!(cache.len(), 2);
}

#[test]
fn cache_invalidates_directory_by_dropping_payload() {
    let mut cache = DirectoryCache::new(2);
    let payload = entries(&["a"]);
    assert!(cache.insert_fresh("/tmp/a", Arc::clone(&payload)).is_some());

    assert!(cache.mark_stale(Path::new("/tmp/a")));

    assert!(cache.get(Path::new("/tmp/a")).is_none());
    assert_eq!(cache.cached_entry_count(), 0);
    assert_eq!(cache.stats().stale_invalidations, 1);
}

#[test]
fn cache_applies_watcher_delta_to_fresh_payload() {
    let root = temp_root("delta");
    fs::write(root.join("a.txt"), b"a").unwrap();
    let mut cache = DirectoryCache::new(2);
    assert!(cache.insert_fresh(&root, entries(&["a.txt"])).is_some());

    fs::write(root.join("b.txt"), b"b").unwrap();
    assert!(cache.apply_items_added(&root, &[entry("b.txt")]));
    let snapshot = cache.get_fresh(&root).unwrap();
    assert_eq!(entry_names(snapshot.entries()), vec!["a.txt", "b.txt"]);

    fs::rename(root.join("b.txt"), root.join("c.txt")).unwrap();
    assert!(cache.apply_items_refreshed(
        &root,
        &[RefreshPair {
            old_path: root.join("b.txt"),
            entry: Some(entry("c.txt")),
        }],
    ));
    let snapshot = cache.get_fresh(&root).unwrap();
    assert_eq!(entry_names(snapshot.entries()), vec!["a.txt", "c.txt"]);

    fs::remove_file(root.join("a.txt")).unwrap();
    assert!(cache.apply_items_deleted(&root, &[root.join("a.txt")]));
    let snapshot = cache.get_fresh(&root).unwrap();
    assert_eq!(entry_names(snapshot.entries()), vec!["c.txt"]);
    assert_eq!(cache.cached_entry_count(), 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn cache_merges_added_batch_without_resorting_existing_entries() {
    let root = temp_root("merge-added");
    fs::write(root.join("a.txt"), b"a").unwrap();
    let mut cache = DirectoryCache::new(2);
    assert!(
        cache
            .insert_fresh(&root, entries(&["b.txt", "d.txt"]))
            .is_some()
    );

    assert!(cache.apply_items_added(&root, &[entry("e.txt"), entry("a.txt")]));

    let snapshot = cache.get_fresh(&root).unwrap();
    assert_eq!(
        entry_names(snapshot.entries()),
        vec!["a.txt", "b.txt", "d.txt", "e.txt"]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn cache_removes_payload_when_delta_exceeds_entry_budget() {
    let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
        max_dirs: 2,
        max_entries: 2,
        max_entries_per_dir: 2,
    });
    assert!(cache.insert_fresh("/tmp/a", entries(&["a"])).is_some());

    assert!(!cache.apply_items_added(Path::new("/tmp/a"), &[entry("b"), entry("c")]));

    assert!(cache.get(Path::new("/tmp/a")).is_none());
    assert_eq!(cache.cached_entry_count(), 0);
    assert_eq!(cache.stats().skipped_large_directories, 1);
}

#[test]
fn fresh_lookup_marks_cache_stale_when_directory_metadata_changes() {
    let root = temp_root("freshness");
    let mut cache = DirectoryCache::new(2);
    let payload = entries(&["a"]);
    assert!(cache.insert_fresh(&root, Arc::clone(&payload)).is_some());

    assert!(cache.get_fresh(&root).is_some());
    std::thread::sleep(Duration::from_millis(20));
    fs::write(root.join("new.txt"), b"changed").unwrap();

    assert!(cache.get_fresh(&root).is_none());
    assert!(cache.get(&root).is_none());
    assert_eq!(cache.cached_entry_count(), 0);
    assert_eq!(cache.stats().stale_invalidations, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn cache_evicts_to_total_entry_budget() {
    let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
        max_dirs: 8,
        max_entries: 3,
        max_entries_per_dir: 8,
    });

    assert!(
        cache
            .insert_fresh("/tmp/a", entries(&["a1", "a2"]))
            .is_some()
    );
    assert_eq!(cache.cached_entry_count(), 2);
    assert!(
        cache
            .insert_fresh("/tmp/b", entries(&["b1", "b2"]))
            .is_some()
    );

    assert!(cache.get(Path::new("/tmp/a")).is_none());
    assert!(cache.get(Path::new("/tmp/b")).is_some());
    assert_eq!(cache.cached_entry_count(), 2);
    assert_eq!(cache.stats().evicted_directories, 1);
}

#[test]
fn cache_does_not_retain_large_directory_payloads() {
    let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
        max_dirs: 8,
        max_entries: 16,
        max_entries_per_dir: 2,
    });

    assert!(cache.can_store_entry_count(2));
    assert!(!cache.can_store_entry_count(3));
    assert!(
        cache
            .insert_fresh("/tmp/large", entries(&["a", "b", "c"]))
            .is_none()
    );

    assert!(cache.get(Path::new("/tmp/large")).is_none());
    assert_eq!(cache.cached_entry_count(), 0);
    assert_eq!(cache.stats().skipped_large_directories, 1);
}

#[test]
fn cache_debug_snapshot_reports_cached_and_uncached_directory_summaries() {
    let mut cache = DirectoryCache::with_limits(DirectoryCacheLimits {
        max_dirs: 2,
        max_entries: 4,
        max_entries_per_dir: 2,
    });

    assert!(
        cache
            .insert_fresh("/tmp/small", entries(&["a", "b"]))
            .is_some()
    );
    assert!(cache.record_uncached_directory(Path::new("/tmp/large"), 3));

    let snapshot = cache.debug_snapshot();

    assert_eq!(snapshot.limits().max_entries_per_dir, 2);
    assert_eq!(snapshot.stats().cached_entries, 2);
    assert_eq!(snapshot.stats().skipped_large_directories, 1);
    assert_eq!(snapshot.cached_directories().len(), 1);
    assert_eq!(
        snapshot.cached_directories()[0].path(),
        Path::new("/tmp/small")
    );
    assert_eq!(snapshot.cached_directories()[0].entry_count(), 2);
    assert_eq!(snapshot.skipped_large_directories().len(), 1);
    assert_eq!(
        snapshot.skipped_large_directories()[0].path(),
        Path::new("/tmp/large")
    );
    assert_eq!(snapshot.skipped_large_directories()[0].entry_count(), 3);

    assert!(cache.remove(Path::new("/tmp/large")));
    assert!(
        cache
            .debug_snapshot()
            .skipped_large_directories()
            .is_empty()
    );
}
