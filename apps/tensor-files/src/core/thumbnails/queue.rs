use super::*;

impl ThumbnailRequestQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn enqueue(&mut self, request: ThumbnailRequest) -> bool {
        let key = request.key();
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => false,
            Some(ThumbnailRequestPriority::Deferred) => {
                if request.priority != ThumbnailRequestPriority::Visible {
                    return false;
                }
                self.deferred.retain(|existing| existing.key() != key);
                self.visible.push_back(request);
                self.pending.insert(key, ThumbnailRequestPriority::Visible);
                true
            }
            None => {
                let priority = request.priority;
                match priority {
                    ThumbnailRequestPriority::Visible => self.visible.push_back(request),
                    ThumbnailRequestPriority::Deferred => self.deferred.push_back(request),
                }
                self.pending.insert(key, priority);
                true
            }
        }
    }

    pub fn contains(&self, request: &ThumbnailRequest) -> bool {
        self.pending.contains_key(&request.key())
    }

    pub fn enqueue_path(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> bool {
        ThumbnailRequest::new(pane_id, generation, item_id, path, priority)
            .is_some_and(|request| self.enqueue(request))
    }

    pub fn enqueue_entry_metadata(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        modified_secs: u64,
        priority: ThumbnailRequestPriority,
    ) -> bool {
        ThumbnailRequest::from_entry_metadata(
            pane_id,
            generation,
            item_id,
            path,
            modified_secs,
            priority,
        )
        .is_some_and(|request| self.enqueue(request))
    }

    pub fn pop_next(&mut self) -> Option<ThumbnailRequest> {
        let request = self
            .visible
            .pop_front()
            .or_else(|| self.deferred.pop_front())?;
        self.pending.remove(&request.key());
        Some(request)
    }

    pub fn cancel_stale_generations(
        &mut self,
        pane_id: PaneId,
        current_generation: Generation,
    ) -> usize {
        self.remove_matching(|request| {
            request.pane_id == pane_id && request.generation != current_generation
        })
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) -> usize {
        self.remove_matching(|request| request.pane_id == pane_id)
    }

    pub fn cancel_deferred_matching(
        &mut self,
        predicate: impl Fn(&ThumbnailRequest) -> bool,
    ) -> Vec<ThumbnailRequest> {
        let mut removed = Vec::new();
        self.deferred.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        for request in &removed {
            self.pending.remove(&request.key());
        }
        removed
    }

    pub fn cancel_matching(
        &mut self,
        predicate: impl Fn(&ThumbnailRequest) -> bool,
    ) -> Vec<ThumbnailRequest> {
        let mut removed = Vec::new();
        self.visible.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        self.deferred.retain(|request| {
            if predicate(request) {
                removed.push(request.clone());
                false
            } else {
                true
            }
        });
        for request in &removed {
            self.pending.remove(&request.key());
        }
        removed
    }

    fn remove_matching(&mut self, predicate: impl Fn(&ThumbnailRequest) -> bool) -> usize {
        let mut removed = remove_matching_from_queue(&mut self.visible, &predicate);
        removed.extend(remove_matching_from_queue(&mut self.deferred, &predicate));
        for key in &removed {
            self.pending.remove(key);
        }
        removed.len()
    }
}
