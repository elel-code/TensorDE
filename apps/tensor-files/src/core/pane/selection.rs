use super::*;

impl SelectionState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selected_ids(&self) -> &[ItemId] {
        &self.selected_ids
    }

    pub fn anchor_id(&self) -> Option<ItemId> {
        self.anchor_id
    }

    pub fn active_id(&self) -> Option<ItemId> {
        self.active_id
    }

    pub fn len(&self) -> usize {
        self.selected_ids.len()
    }

    pub fn count_for_model(&self, model_len: usize) -> usize {
        if self.all_selected {
            model_len.saturating_sub(self.excluded_ids.len())
        } else {
            self.selected_ids.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.all_selected && self.selected_ids.is_empty()
    }

    pub fn is_all_selected(&self) -> bool {
        self.all_selected
    }

    pub fn is_excluded(&self, id: ItemId) -> bool {
        self.excluded_ids.contains(&id)
    }

    pub fn is_selected(&self, id: ItemId) -> bool {
        if self.all_selected {
            !self.is_excluded(id)
        } else {
            self.selected_ids.contains(&id)
        }
    }

    pub fn is_only_selected(&self, id: ItemId) -> bool {
        !self.all_selected
            && self.excluded_ids.is_empty()
            && self.selected_ids.as_slice() == [id]
            && self.anchor_id == Some(id)
            && self.active_id == Some(id)
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.anchor_id = None;
        self.active_id = None;
        self.bump_revision();
    }

    pub fn select_only(&mut self, id: ItemId) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.selected_ids.push(id);
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        self.bump_revision();
    }

    pub fn toggle(&mut self, id: ItemId) -> bool {
        if self.all_selected {
            self.anchor_id = Some(id);
            self.active_id = Some(id);
            if let Some(index) = self
                .excluded_ids
                .iter()
                .position(|excluded| *excluded == id)
            {
                self.excluded_ids.remove(index);
                self.bump_revision();
                return true;
            }
            self.excluded_ids.push(id);
            self.bump_revision();
            return false;
        }
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        if let Some(index) = self
            .selected_ids
            .iter()
            .position(|selected| *selected == id)
        {
            self.selected_ids.remove(index);
            self.bump_revision();
            false
        } else {
            self.selected_ids.push(id);
            self.bump_revision();
            true
        }
    }

    pub fn replace(&mut self, ids: Vec<ItemId>) {
        self.all_selected = false;
        self.excluded_ids.clear();
        let mut seen = BTreeSet::new();
        self.selected_ids = ids
            .into_iter()
            .filter(|id| id.is_assigned() && seen.insert(*id))
            .collect();
        if self
            .anchor_id
            .is_none_or(|anchor| !self.selected_ids.contains(&anchor))
        {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self
            .active_id
            .is_none_or(|active| !self.selected_ids.contains(&active))
        {
            self.active_id = self.selected_ids.first().copied();
        }
        self.bump_revision();
    }

    pub fn select_all(&mut self, anchor_id: Option<ItemId>) {
        if anchor_id.is_none() {
            self.clear();
            return;
        }
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = true;
        self.anchor_id = anchor_id;
        self.active_id = anchor_id;
        self.bump_revision();
    }

    pub fn replace_range(&mut self, anchor_id: ItemId, ids: Vec<ItemId>) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
    }

    pub fn replace_range_with_active(
        &mut self,
        anchor_id: ItemId,
        active_id: ItemId,
        ids: Vec<ItemId>,
    ) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
        self.active_id = Some(active_id);
    }

    pub fn retain_existing_by(
        &mut self,
        mut exists: impl FnMut(ItemId) -> bool,
        fallback_id: Option<ItemId>,
    ) {
        if self.all_selected {
            let before_excluded_len = self.excluded_ids.len();
            let before_anchor = self.anchor_id;
            let before_active = self.active_id;
            self.excluded_ids.retain(|id| exists(*id));
            if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
                self.anchor_id = fallback_id;
            }
            if self.active_id.is_some_and(|active| !exists(active)) {
                self.active_id = fallback_id;
            }
            if fallback_id.is_none() {
                self.clear();
            }
            if self.excluded_ids.len() != before_excluded_len
                || self.anchor_id != before_anchor
                || self.active_id != before_active
            {
                self.bump_revision();
            }
            return;
        }

        let before_selected_len = self.selected_ids.len();
        let before_anchor = self.anchor_id;
        let before_active = self.active_id;
        self.selected_ids.retain(|id| exists(*id));
        if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self.active_id.is_some_and(|active| !exists(active)) {
            self.active_id = self.selected_ids.first().copied();
        }
        if self.selected_ids.len() != before_selected_len
            || self.anchor_id != before_anchor
            || self.active_id != before_active
        {
            self.bump_revision();
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}
