use super::entries::{EntryMetadataRole, ItemId};
use super::mime::{MimeDatabase, mime_magic_resolution_required, read_mime_magic};
use super::model::DirectoryModel;
use super::network::is_network_path;
use super::pane::{Generation, PaneId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetadataRoleWorkKey {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path_hash: u64,
    pub modified_secs: Option<u64>,
    pub mime_type: Option<String>,
}

impl MetadataRoleWorkKey {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: &MetadataRoleCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path_hash: stable_hash(&candidate.path),
            modified_secs: candidate.modified_secs,
            mime_type: candidate.mime_type.clone(),
        }
    }

    pub fn from_request(request: &MetadataRoleRequest) -> Self {
        Self {
            pane_id: request.pane_id,
            generation: request.generation,
            item_id: request.item_id,
            path_hash: stable_hash(&request.path),
            modified_secs: request.modified_secs,
            mime_type: request.mime_type.clone(),
        }
    }

    pub fn from_result(result: &MetadataRoleResult) -> Self {
        Self {
            pane_id: result.pane_id,
            generation: result.generation,
            item_id: result.item_id,
            path_hash: stable_hash(&result.path),
            modified_secs: result.role.as_ref().and_then(|role| role.modified_secs),
            mime_type: result
                .role
                .as_ref()
                .and_then(|role| role.mime_type.as_ref().map(|mime| mime.to_string())),
        }
    }
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleCandidate {
    pub item_id: ItemId,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_secs: Option<u64>,
    pub mime_type: Option<String>,
    pub mime_magic_checked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
    size_bytes: u64,
    modified_secs: Option<u64>,
    mime_type: Option<String>,
}

impl MetadataRoleRequest {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: MetadataRoleCandidate,
    ) -> Option<Self> {
        if is_network_path(&candidate.path) {
            return None;
        }
        if !mime_magic_resolution_required(
            false,
            candidate.size_bytes,
            candidate.mime_type.as_deref(),
            candidate.mime_magic_checked,
        ) {
            return None;
        }
        Some(Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            path: candidate.path,
            size_bytes: candidate.size_bytes,
            modified_secs: candidate.modified_secs,
            mime_type: candidate.mime_type,
        })
    }

    pub fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn item_id(&self) -> ItemId {
        self.item_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn modified_secs(&self) -> Option<u64> {
        self.modified_secs
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleResult {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path: PathBuf,
    pub role: Option<EntryMetadataRole>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRoleBatch {
    pub requests: Vec<MetadataRoleRequest>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MetadataRolePriority {
    Deferred,
    Visible,
}

#[derive(Debug, Default)]
pub struct MetadataRoleScheduler {
    visible: VecDeque<MetadataRoleRequest>,
    deferred: VecDeque<MetadataRoleRequest>,
    seen: HashMap<MetadataRoleWorkKey, MetadataRolePriority>,
    active: HashMap<MetadataRoleWorkKey, MetadataRolePriority>,
    role_batch_pending: bool,
}

impl MetadataRoleScheduler {
    pub fn queue_candidates(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = MetadataRoleCandidate>,
    ) -> bool {
        self.queue_candidates_with_priority(
            pane_id,
            generation,
            candidates,
            MetadataRolePriority::Visible,
        )
    }

    pub fn queue_candidates_with_priority(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = MetadataRoleCandidate>,
        priority: MetadataRolePriority,
    ) -> bool {
        let mut keep = HashSet::new();
        let mut pending = Vec::new();
        for candidate in candidates {
            let key = MetadataRoleWorkKey::from_candidate(pane_id, generation, &candidate);
            keep.insert(key.clone());
            let Some(request) = MetadataRoleRequest::from_candidate(pane_id, generation, candidate)
            else {
                continue;
            };
            pending.push((key, request));
        }
        if priority == MetadataRolePriority::Visible {
            self.prune_visible_for_snapshot(pane_id, generation, &keep);
        }

        let mut queued = false;
        for (key, request) in pending {
            if self
                .seen
                .get(&key)
                .is_some_and(|queued_priority| *queued_priority >= priority)
            {
                continue;
            }
            self.seen.insert(key.clone(), priority);
            match priority {
                MetadataRolePriority::Visible => {
                    self.deferred
                        .retain(|queued| MetadataRoleWorkKey::from_request(queued) != key);
                    self.visible.push_back(request);
                }
                MetadataRolePriority::Deferred => self.deferred.push_back(request),
            }
            queued = true;
        }
        queued
    }

    pub fn start_role_batch(&mut self, batch_size: usize) -> Option<MetadataRoleBatch> {
        if self.role_batch_pending || (self.visible.is_empty() && self.deferred.is_empty()) {
            return None;
        }
        let mut requests = Vec::new();
        let mut active = HashMap::new();
        while requests.len() < batch_size {
            let Some((request, priority)) = self.pop_next_queued_request() else {
                break;
            };
            active.insert(MetadataRoleWorkKey::from_request(&request), priority);
            requests.push(request);
        }
        if requests.is_empty() {
            None
        } else {
            self.role_batch_pending = true;
            self.active = active;
            Some(MetadataRoleBatch { requests })
        }
    }

    pub fn finish_role_batch(&mut self) {
        self.role_batch_pending = false;
        for (key, _) in self.active.drain() {
            self.seen.remove(&key);
        }
    }

    pub fn finish_role_batch_with_results(&mut self, results: &[MetadataRoleResult]) {
        let failed = results
            .iter()
            .filter(|result| result.role.is_none())
            .map(MetadataRoleWorkKey::from_result)
            .collect::<HashSet<_>>();
        self.role_batch_pending = false;
        for (key, _) in self.active.drain() {
            if !failed.contains(&key) {
                self.seen.remove(&key);
            }
        }
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) {
        self.visible.retain(|request| request.pane_id != pane_id);
        self.deferred.retain(|request| request.pane_id != pane_id);
        self.seen.retain(|key, _| key.pane_id != pane_id);
        self.active.retain(|key, _| key.pane_id != pane_id);
    }

    pub fn cancel_stale_pane_generations(&mut self, pane_id: PaneId, generation: Generation) {
        self.visible
            .retain(|request| request.pane_id != pane_id || request.generation == generation);
        self.deferred
            .retain(|request| request.pane_id != pane_id || request.generation == generation);
        self.seen
            .retain(|key, _| key.pane_id != pane_id || key.generation == generation);
        self.active
            .retain(|key, _| key.pane_id != pane_id || key.generation == generation);
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.deferred.is_empty() && !self.role_batch_pending
    }

    pub fn has_visible_pending(&self) -> bool {
        self.visible.iter().any(|request| {
            self.seen.get(&MetadataRoleWorkKey::from_request(request))
                == Some(&MetadataRolePriority::Visible)
        }) || self
            .active
            .values()
            .any(|priority| *priority == MetadataRolePriority::Visible)
    }

    pub fn priority_for_result(&self, result: &MetadataRoleResult) -> Option<MetadataRolePriority> {
        let path_hash = stable_hash(&result.path);
        self.active
            .iter()
            .filter(|(key, _)| metadata_work_key_matches_result(key, result, path_hash))
            .map(|(_, priority)| *priority)
            .chain(
                self.seen
                    .iter()
                    .filter(|(key, _)| metadata_work_key_matches_result(key, result, path_hash))
                    .map(|(_, priority)| *priority),
            )
            .max()
    }

    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.visible.len() + self.deferred.len()
    }

    #[cfg(test)]
    pub(crate) fn seen_len(&self) -> usize {
        self.seen.len()
    }

    fn pop_next_queued_request(&mut self) -> Option<(MetadataRoleRequest, MetadataRolePriority)> {
        while let Some(request) = self.visible.pop_front() {
            let key = MetadataRoleWorkKey::from_request(&request);
            if self.seen.get(&key) == Some(&MetadataRolePriority::Visible) {
                return Some((request, MetadataRolePriority::Visible));
            }
        }
        while let Some(request) = self.deferred.pop_front() {
            let key = MetadataRoleWorkKey::from_request(&request);
            if self.seen.get(&key) == Some(&MetadataRolePriority::Deferred) {
                return Some((request, MetadataRolePriority::Deferred));
            }
        }
        None
    }

    fn prune_visible_for_snapshot(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        keep: &HashSet<MetadataRoleWorkKey>,
    ) {
        let mut removed = Vec::new();
        self.visible.retain(|request| {
            if request.pane_id != pane_id || request.generation != generation {
                return true;
            }
            let key = MetadataRoleWorkKey::from_request(request);
            if keep.contains(&key) {
                true
            } else {
                removed.push(key);
                false
            }
        });
        for key in removed {
            if self.seen.get(&key) == Some(&MetadataRolePriority::Visible) {
                self.seen.remove(&key);
            }
        }
    }
}

fn metadata_work_key_matches_result(
    key: &MetadataRoleWorkKey,
    result: &MetadataRoleResult,
    result_path_hash: u64,
) -> bool {
    key.pane_id == result.pane_id
        && key.generation == result.generation
        && key.item_id == result.item_id
        && key.path_hash == result_path_hash
}

pub fn metadata_role_results_for_requests(
    requests: Vec<MetadataRoleRequest>,
) -> Vec<MetadataRoleResult> {
    requests
        .into_iter()
        .map(metadata_role_result_for_request)
        .collect()
}

pub fn metadata_role_result_for_request(request: MetadataRoleRequest) -> MetadataRoleResult {
    let mime_type = read_mime_magic(request.path())
        .ok()
        .flatten()
        .and_then(|magic| MimeDatabase::shared().mime_for_path(request.path(), false, Some(&magic)))
        .or_else(|| request.mime_type().map(Arc::from));
    let role = Some(EntryMetadataRole {
        size_bytes: request.size_bytes(),
        modified_secs: request.modified_secs(),
        mime_type,
        mime_magic_checked: true,
    });
    MetadataRoleResult {
        pane_id: request.pane_id,
        generation: request.generation,
        item_id: request.item_id,
        path: request.path,
        role,
    }
}

pub fn apply_metadata_role_result_to_model(
    model: &mut DirectoryModel,
    result: MetadataRoleResult,
) -> bool {
    let Some(role) = result.role else {
        return false;
    };
    let Some(index) = model.index_of_id(result.item_id) else {
        return false;
    };
    let Some(entry) = model.get(index) else {
        return false;
    };
    if entry.is_dir
        || entry.effective_modified_secs() != role.modified_secs
        || model.path_for_index(index).as_deref() != Some(result.path.as_path())
    {
        return false;
    }
    !model
        .set_metadata_role(result.item_id, &result.path, role)
        .is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entries::{Entry, EntryData};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn metadata_role_scheduler_queues_once_and_clears_pane_work() {
        let pane_id = PaneId(1);
        let generation = Generation(1);
        let mut scheduler = MetadataRoleScheduler::default();
        let candidate = metadata_candidate(
            ItemId(1),
            PathBuf::from("/tmp/tensor-files-metadata/payload"),
        );

        assert!(scheduler.queue_candidates(pane_id, generation, vec![candidate.clone()]));
        assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(scheduler.start_role_batch(8).is_none());
        scheduler.finish_role_batch();
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.seen_len(), 0);
    }

    #[test]
    fn metadata_role_scheduler_prunes_invisible_queued_requests() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-prune");
        let first = metadata_candidate(ItemId(1), root.join("first"));
        let second = metadata_candidate(ItemId(2), root.join("second"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![first.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![second.clone()]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);
        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests[0].item_id(), second.item_id);
        scheduler.finish_role_batch();
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.seen_len(), 0);
    }

    #[test]
    fn metadata_role_scheduler_promotes_visible_work_over_deferred() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-priority");
        let deferred = metadata_candidate(ItemId(1), root.join("deferred"));
        let visible = metadata_candidate(ItemId(2), root.join("visible"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates_with_priority(
            PaneId(1),
            Generation(1),
            vec![deferred.clone()],
            MetadataRolePriority::Deferred,
        ));
        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![visible.clone()]));

        let batch = scheduler.start_role_batch(2).unwrap();
        assert_eq!(batch.requests[0].item_id(), visible.item_id);
        assert_eq!(batch.requests[1].item_id(), deferred.item_id);
        assert!(scheduler.has_visible_pending());
        scheduler.finish_role_batch();
        assert!(!scheduler.has_visible_pending());
    }

    #[test]
    fn metadata_role_scheduler_ignores_deferred_only_work_for_visible_pending() {
        let candidate = metadata_candidate(
            ItemId(1),
            PathBuf::from("/tmp/tensor-files-metadata-deferred-only/payload"),
        );
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates_with_priority(
            PaneId(1),
            Generation(1),
            vec![candidate],
            MetadataRolePriority::Deferred,
        ));
        assert!(!scheduler.has_visible_pending());

        let batch = scheduler.start_role_batch(1).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert!(!scheduler.has_visible_pending());
    }

    #[test]
    fn metadata_role_scheduler_promotes_same_key_from_deferred_to_visible() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-promote");
        let candidate = metadata_candidate(ItemId(1), root.join("payload"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates_with_priority(
            PaneId(1),
            Generation(1),
            vec![candidate.clone()],
            MetadataRolePriority::Deferred,
        ));
        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![candidate.clone()]));

        assert_eq!(scheduler.queued_len(), 1);
        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(batch.requests[0].item_id(), candidate.item_id);
    }

    #[test]
    fn metadata_role_scheduler_visible_snapshot_keeps_deferred_background_work() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-visible-keeps-deferred");
        let deferred = metadata_candidate(ItemId(1), root.join("deferred"));
        let visible = metadata_candidate(ItemId(2), root.join("visible"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates_with_priority(
            PaneId(1),
            Generation(1),
            vec![deferred.clone()],
            MetadataRolePriority::Deferred,
        ));
        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![visible.clone()]));
        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![visible.clone()]));

        let batch = scheduler.start_role_batch(8).unwrap();
        assert_eq!(batch.requests.len(), 2);
        assert_eq!(batch.requests[0].item_id(), visible.item_id);
        assert_eq!(batch.requests[1].item_id(), deferred.item_id);
    }

    #[test]
    fn metadata_role_scheduler_releases_finished_active_keys_but_keeps_queued_keys() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-active");
        let first = metadata_candidate(ItemId(1), root.join("first"));
        let second = metadata_candidate(ItemId(2), root.join("second"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(
            PaneId(1),
            Generation(1),
            vec![first.clone(), second.clone()]
        ));
        assert_eq!(scheduler.seen_len(), 2);

        let batch = scheduler.start_role_batch(1).unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(scheduler.queued_len(), 1);
        scheduler.finish_role_batch();

        assert_eq!(scheduler.seen_len(), 1);
        assert!(!scheduler.queue_candidates(PaneId(1), Generation(1), vec![second]));
        assert_eq!(scheduler.queued_len(), 1);
    }

    #[test]
    fn metadata_role_scheduler_releases_active_key_after_batch_finish() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-failed");
        let candidate = metadata_candidate(ItemId(1), root.join("missing"));
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![candidate.clone()]));
        let _batch = scheduler.start_role_batch(8).unwrap();
        scheduler.finish_role_batch();

        assert!(scheduler.queue_candidates(PaneId(1), Generation(1), vec![candidate]));
        assert_eq!(scheduler.queued_len(), 1);
        assert_eq!(scheduler.seen_len(), 1);
    }

    #[test]
    fn metadata_role_result_applies_only_to_matching_model_item_path_and_mtime() {
        let root = PathBuf::from("/tmp/tensor-files-metadata-result");
        let mut model = DirectoryModel::for_directory(root.clone());
        model.replace_listing(
            root.clone(),
            Arc::new(vec![Entry::new(EntryData {
                name: Arc::from("payload"),
                name_width_units: 7,
                target_path: None,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from(super::super::mime::GENERIC_BINARY_MIME)),
                mime_magic_checked: false,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            })]),
        );
        let item_id = model.entries()[0].id;
        let role = EntryMetadataRole {
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        };

        assert!(!apply_metadata_role_result_to_model(
            &mut model,
            MetadataRoleResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("other"),
                role: Some(role.clone()),
            },
        ));
        assert_eq!(
            model.entries()[0].effective_mime_type().map(Arc::as_ref),
            Some(super::super::mime::GENERIC_BINARY_MIME)
        );

        assert!(!apply_metadata_role_result_to_model(
            &mut model,
            MetadataRoleResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                role: Some(EntryMetadataRole {
                    size_bytes: 12,
                    modified_secs: Some(43),
                    mime_type: Some(Arc::from("text/plain")),
                    mime_magic_checked: true,
                }),
            },
        ));
        assert_eq!(
            model.entries()[0].effective_mime_type().map(Arc::as_ref),
            Some(super::super::mime::GENERIC_BINARY_MIME)
        );

        assert!(apply_metadata_role_result_to_model(
            &mut model,
            MetadataRoleResult {
                pane_id: PaneId(1),
                generation: Generation(1),
                item_id,
                path: root.join("payload"),
                role: Some(role),
            },
        ));
        assert_eq!(model.entries()[0].effective_size_bytes(), 12);
        assert_eq!(model.entries()[0].effective_modified_secs(), Some(42));
        assert_eq!(
            model.entries()[0].effective_mime_type().map(Arc::as_ref),
            Some("text/plain")
        );
    }

    #[test]
    fn metadata_role_result_reads_magic_without_restating_file() {
        let root = std::env::temp_dir().join(format!(
            "tensor-files-metadata-role-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("payload");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();

        let result = metadata_role_result_for_request(MetadataRoleRequest {
            pane_id: PaneId(1),
            generation: Generation(2),
            item_id: ItemId(3),
            path: path.clone(),
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(super::super::mime::GENERIC_BINARY_MIME.to_string()),
        });

        assert_eq!(result.pane_id, PaneId(1));
        assert_eq!(result.generation, Generation(2));
        assert_eq!(result.item_id, ItemId(3));
        assert_eq!(result.path, path);
        let role = result.role.unwrap();
        assert_eq!(role.size_bytes, 12);
        assert_eq!(role.modified_secs, Some(42));
        assert_eq!(role.mime_type.as_deref(), Some("image/png"));
        assert!(role.mime_magic_checked);

        let _ = std::fs::remove_dir_all(root);
    }

    fn metadata_candidate(item_id: ItemId, path: PathBuf) -> MetadataRoleCandidate {
        MetadataRoleCandidate {
            item_id,
            path,
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(super::super::mime::GENERIC_BINARY_MIME.to_string()),
            mime_magic_checked: false,
        }
    }
}
