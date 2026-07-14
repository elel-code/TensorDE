
#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(
        dpb_slots: u32,
        max_short_term_references: u32,
        max_frame_num: u32,
        gaps_in_frame_num_allowed: bool,
    ) -> Self {
        let dpb_slots = dpb_slots.max(1);
        Self {
            dpb_slots,
            max_short_term_references: max_short_term_references.max(1),
            max_frame_num: max_frame_num.max(1),
            gaps_in_frame_num_allowed,
            previous_reference_frame_num: None,
            short_term_references: BTreeMap::new(),
            long_term_references: BTreeMap::new(),
            slot_to_reference_key: vec![None; dpb_slots as usize],
            short_term_reference_order: Vec::new(),
            next_output_slot: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn reset(&mut self) {
        self.previous_reference_frame_num = None;
        self.short_term_references.clear();
        self.long_term_references.clear();
        self.slot_to_reference_key.fill(None);
        self.short_term_reference_order.clear();
        self.next_output_slot = 0;
    }

    pub(in crate::renderer::native_vulkan) fn choose_output_slot(&mut self) -> u32 {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if self
                .slot_to_reference_key
                .get(slot as usize)
                .is_none_or(Option::is_none)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        let slot = self.next_output_slot % self.dpb_slots;
        self.next_output_slot = (slot + 1) % self.dpb_slots;
        slot
    }

    pub(in crate::renderer::native_vulkan) fn active_reference_count(&self) -> usize {
        self.short_term_references
            .len()
            .saturating_add(self.long_term_references.len())
    }

    pub(in crate::renderer::native_vulkan) fn enforce_sliding_reference_window(
        &mut self,
    ) -> Vec<(u16, u32)> {
        let mut dropped = Vec::new();
        while self.active_reference_count() > self.max_short_term_references as usize {
            let Some(old_key) = self.short_term_reference_order.first().copied() else {
                break;
            };
            self.short_term_reference_order.remove(0);
            if let Some(old_slot) = self.remove_short_term_reference(old_key) {
                dropped.push((old_key.frame_num, old_slot));
            }
        }
        dropped
    }

    pub(in crate::renderer::native_vulkan) fn infer_non_existing_short_term_references(
        &mut self,
        current_frame_num: u16,
    ) -> Result<NativeVulkanH264InferredNonExistingPlan, String> {
        let Some(previous_frame_num) = self.previous_reference_frame_num else {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        };
        let max_frame_num = self.max_frame_num.max(1);
        let current_frame_num_u32 = u32::from(current_frame_num);
        if current_frame_num_u32 >= max_frame_num {
            return Err(format!(
                "H.264 current frame_num {current_frame_num} is outside max_frame_num {max_frame_num}"
            ));
        }
        let previous_frame_num_u32 = u32::from(previous_frame_num) % max_frame_num;
        if previous_frame_num_u32 == current_frame_num_u32 {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        }
        let mut frame_num = (previous_frame_num_u32 + 1) % max_frame_num;
        if frame_num == current_frame_num_u32 {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        }
        if !self.gaps_in_frame_num_allowed {
            return Err(format!(
                "H.264 frame_num gap from {previous_frame_num} to {current_frame_num} but SPS gaps_in_frame_num_value_allowed_flag is false"
            ));
        }

        let mut plan = NativeVulkanH264InferredNonExistingPlan::default();
        let mut guard = 0u32;
        while frame_num != current_frame_num_u32 && guard < max_frame_num {
            let inferred_frame_num = u16::try_from(frame_num).map_err(|_| {
                format!(
                    "H.264 inferred non-existing frame_num {frame_num} exceeds supported u16 range"
                )
            })?;
            let inferred_key = NativeVulkanH264ShortTermPictureKey::frame(inferred_frame_num);
            plan.frame_nums.push(inferred_frame_num);
            let slot = self.choose_output_slot();
            self.clear_slot_for_inference(slot, &mut plan);
            let reference_state = NativeVulkanH264DpbReferenceState {
                source_access_unit_index: None,
                dpb_slot: slot,
                pic_order_cnt_val: i32::from(inferred_frame_num),
                pic_order_cnt: [i32::from(inferred_frame_num); 2],
                frame_num: inferred_frame_num,
                field_kind: inferred_key.field_kind,
                non_existing: true,
            };
            self.short_term_references
                .insert(inferred_key, reference_state);
            if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
                *slot_ref = Some(NativeVulkanH264DpbSlotKey::ShortTerm(inferred_key));
            }
            self.short_term_reference_order
                .retain(|existing| *existing != inferred_key);
            self.short_term_reference_order.push(inferred_key);
            for (old_frame_num, old_slot) in self.enforce_sliding_reference_window() {
                if !plan.dropped_short_term_frame_nums.contains(&old_frame_num) {
                    plan.dropped_short_term_frame_nums.push(old_frame_num);
                }
                if !plan.dropped_reference_slots.contains(&old_slot) {
                    plan.dropped_reference_slots.push(old_slot);
                }
            }
            frame_num = (frame_num + 1) % max_frame_num;
            guard = guard.saturating_add(1);
        }
        if frame_num != current_frame_num_u32 {
            return Err(format!(
                "H.264 frame_num gap inference from {previous_frame_num} to {current_frame_num} did not converge within max_frame_num {}",
                max_frame_num
            ));
        }
        plan.references = plan
            .frame_nums
            .iter()
            .filter_map(|frame_num| {
                native_vulkan_h264_first_short_term_key_for_frame_num(
                    &self.short_term_references,
                    *frame_num,
                )
                .and_then(|key| {
                    self.short_term_references.get(&key).and_then(|reference| {
                        reference.non_existing.then_some(
                            NativeVulkanH264InferredNonExistingReferenceSnapshot {
                                frame_num: reference.frame_num,
                                field_pic_flag: reference.field_kind.field_pic_flag(),
                                bottom_field_flag: reference.field_kind.bottom_field_flag(),
                                pic_order_cnt_val: reference.pic_order_cnt_val,
                                pic_order_cnt: reference.pic_order_cnt,
                                dpb_slot: reference.dpb_slot,
                            },
                        )
                    })
                })
            })
            .collect();
        Ok(plan)
    }

    pub(in crate::renderer::native_vulkan) fn record_inference_dropped_key(
        &mut self,
        key: NativeVulkanH264DpbSlotKey,
        slot: u32,
        plan: &mut NativeVulkanH264InferredNonExistingPlan,
    ) {
        match key {
            NativeVulkanH264DpbSlotKey::ShortTerm(key) => {
                self.short_term_reference_order
                    .retain(|existing| *existing != key);
                self.short_term_references.remove(&key);
                if !plan.dropped_short_term_frame_nums.contains(&key.frame_num) {
                    plan.dropped_short_term_frame_nums.push(key.frame_num);
                }
            }
            NativeVulkanH264DpbSlotKey::LongTerm(long_term_key) => {
                self.long_term_references.remove(&long_term_key);
                if !plan
                    .dropped_long_term_frame_indices
                    .contains(&long_term_key.frame_idx)
                {
                    plan.dropped_long_term_frame_indices
                        .push(long_term_key.frame_idx);
                }
            }
        }
        if !plan.dropped_reference_slots.contains(&slot) {
            plan.dropped_reference_slots.push(slot);
        }
    }

    pub(in crate::renderer::native_vulkan) fn clear_slot_for_inference(
        &mut self,
        slot: u32,
        plan: &mut NativeVulkanH264InferredNonExistingPlan,
    ) {
        let key = self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten();
        if let Some(key) = key {
            self.record_inference_dropped_key(key, slot, plan);
        }
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = None;
        }
    }

    pub(in crate::renderer::native_vulkan) fn remove_short_term_reference(
        &mut self,
        key: NativeVulkanH264ShortTermPictureKey,
    ) -> Option<u32> {
        self.short_term_reference_order
            .retain(|existing| *existing != key);
        let reference = self.short_term_references.remove(&key)?;
        let slot = reference.dpb_slot;
        if self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten()
            == Some(NativeVulkanH264DpbSlotKey::ShortTerm(key))
            && let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize)
        {
            *slot_ref = None;
        }
        Some(slot)
    }

    pub(in crate::renderer::native_vulkan) fn remove_long_term_reference(
        &mut self,
        long_term_key: NativeVulkanH264LongTermPictureKey,
    ) -> Option<u32> {
        let reference = self.long_term_references.remove(&long_term_key)?;
        let slot = reference.dpb_slot;
        if self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten()
            == Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key))
            && let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize)
        {
            *slot_ref = None;
        }
        Some(slot)
    }

    pub(in crate::renderer::native_vulkan) fn clear_slot(&mut self, slot: u32) {
        let key = self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten();
        match key {
            Some(NativeVulkanH264DpbSlotKey::ShortTerm(key)) => {
                self.short_term_reference_order
                    .retain(|existing| *existing != key);
                self.short_term_references.remove(&key);
            }
            Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key)) => {
                self.long_term_references.remove(&long_term_key);
            }
            None => {}
        }
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = None;
        }
    }

    pub(in crate::renderer::native_vulkan) fn convert_short_term_to_long_term(
        &mut self,
        key: NativeVulkanH264ShortTermPictureKey,
        long_term_frame_idx: u16,
    ) -> Option<(u32, Option<u32>)> {
        let long_term_key =
            NativeVulkanH264LongTermPictureKey::from_short_term(key, long_term_frame_idx);
        let replaced_slot = self.remove_long_term_reference(long_term_key);
        self.short_term_reference_order
            .retain(|existing| *existing != key);
        let reference = self.short_term_references.remove(&key)?;
        let slot = reference.dpb_slot;
        self.long_term_references.insert(long_term_key, reference);
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key));
        }
        Some((slot, replaced_slot))
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        access_unit: &NativeVulkanH264AccessUnitSnapshot,
    ) -> NativeVulkanH264DecodeReferencePlanEntrySnapshot {
        let first_slice = access_unit.first_slice.as_ref();
        let idr = first_slice.is_some_and(|slice| slice.idr);
        if idr {
            self.reset();
        }

        let current_frame_num = first_slice.map(|slice| slice.frame_num);
        let current_pic_order_cnt_val = first_slice.map(|slice| {
            native_vulkan_h264_picture_order_cnt_val(
                slice.field_pic_flag,
                slice.bottom_field_flag,
                slice.pic_order_cnt,
            )
        });
        let current_pic_order_cnt = first_slice.map(|slice| slice.pic_order_cnt);
        let current_long_term_frame_idx = first_slice
            .filter(|slice| slice.idr && slice.long_term_reference_flag)
            .map(|_| 0u16);

        let mut unsupported_reason = access_unit.first_slice_parse_error.clone();
        let requested_l0_reference_count = first_slice
            .and_then(|slice| {
                if slice.is_p || slice.is_b {
                    slice
                        .num_ref_idx_l0_active_minus1
                        .map(|value| value.saturating_add(1))
                } else {
                    Some(0)
                }
            })
            .unwrap_or(0);
        let requested_l1_reference_count = first_slice
            .and_then(|slice| {
                slice
                    .is_b
                    .then(|| slice.num_ref_idx_l1_active_minus1.map(|value| value + 1))
                    .flatten()
            })
            .unwrap_or(0);
        let mut requested_reference_count = if first_slice.is_some_and(|slice| slice.is_p) {
            requested_l0_reference_count
        } else {
            0
        };
        if unsupported_reason.is_none() {
            if let Some(slice) = first_slice {
                unsupported_reason = if !native_vulkan_h264_ref_pic_list_modifications_supported(
                    slice,
                ) {
                    Some("H.264 unsupported reference list modification is not supported by the continuous direct gate".to_owned())
                } else if slice.is_p && requested_reference_count == 0 {
                    Some("H.264 P-slice requested zero active references".to_owned())
                } else if slice.is_b
                    && (requested_l0_reference_count == 0 || requested_l1_reference_count == 0)
                {
                    Some("H.264 B-slice requested zero active L0/L1 references".to_owned())
                } else if !slice.is_intra && !slice.is_p && !slice.is_b {
                    Some(format!(
                        "H.264 slice_type={} is not supported by the first continuous direct gate",
                        slice.slice_type
                    ))
                } else {
                    None
                };
            } else {
                unsupported_reason = Some(format!(
                    "H.264 AU {} has no parsed first slice",
                    access_unit.index
                ));
            }
        }

        let mut inferred_non_existing_plan = NativeVulkanH264InferredNonExistingPlan::default();
        if unsupported_reason.is_none()
            && let (Some(slice), Some(current_frame_num)) = (first_slice, current_frame_num)
            && !slice.idr
            && slice.is_reference
        {
            match self.infer_non_existing_short_term_references(current_frame_num) {
                Ok(plan) => inferred_non_existing_plan = plan,
                Err(err) => unsupported_reason = Some(err),
            }
        }

        let planned_output_slot = if current_frame_num.is_some() {
            self.choose_output_slot()
        } else {
            self.next_output_slot % self.dpb_slots
        };
        let evicted_key = self
            .slot_to_reference_key
            .get(planned_output_slot as usize)
            .copied()
            .flatten();
        let evicted_frame_num = match evicted_key {
            Some(NativeVulkanH264DpbSlotKey::ShortTerm(key)) => Some(key.frame_num),
            _ => None,
        };
        let evicted_long_term_frame_idx = match evicted_key {
            Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key)) => {
                Some(long_term_key.frame_idx)
            }
            _ => None,
        };

        let mut reference_entries = NativeVulkanH264ReferenceListEntries::new();
        if unsupported_reason.is_none()
            && let Some(slice) = first_slice
        {
            if slice.is_b {
                match native_vulkan_h264_b_reference_frame_nums_for_slice(
                    slice,
                    &self.short_term_references,
                    &self.long_term_references,
                    planned_output_slot,
                    self.max_frame_num,
                ) {
                    Ok(entries) => {
                        requested_reference_count = entries.len() as u32;
                        reference_entries = entries;
                    }
                    Err(err) => unsupported_reason = Some(err),
                }
            } else if requested_reference_count > 0 {
                match native_vulkan_h264_reference_frame_nums_for_slice(
                    slice,
                    &self.short_term_references,
                    &self.long_term_references,
                    planned_output_slot,
                    self.max_frame_num,
                ) {
                    Ok(entries) => reference_entries = entries,
                    Err(err) => unsupported_reason = Some(err),
                }
            }
        }
        if unsupported_reason.is_none()
            && requested_reference_count > self.max_short_term_references
        {
            unsupported_reason = Some(format!(
                "H.264 slice requests {requested_reference_count} active references but stream/driver plan keeps {}",
                self.max_short_term_references
            ));
        }
        let mut adaptive_marking_plan = NativeVulkanH264AdaptiveMarkingPlan::default();
        if unsupported_reason.is_none()
            && let Some(slice) = first_slice
            && slice.is_reference
        {
            match native_vulkan_h264_adaptive_marking_plan_for_slice(
                slice,
                &self.short_term_references,
                &self.long_term_references,
                self.max_frame_num,
            ) {
                Ok(plan) => adaptive_marking_plan = plan,
                Err(err) => unsupported_reason = Some(err),
            }
        }

        let references = if unsupported_reason.is_none() && requested_reference_count > 0 {
            reference_entries
                .iter()
                .copied()
                .take(requested_reference_count as usize)
                .filter_map(|entry| {
                    let (
                        used_for_long_term_reference,
                        long_term_frame_idx,
                        long_term_pic_num,
                        reference,
                    ) = match entry {
                        NativeVulkanH264ReferenceListEntry::ShortTerm(key) => {
                            (false, None, None, self.short_term_references.get(&key))
                        }
                        NativeVulkanH264ReferenceListEntry::LongTerm(long_term_key) => (
                            true,
                            Some(long_term_key.frame_idx),
                            first_slice.and_then(|slice| {
                                u16::try_from(native_vulkan_h264_long_term_pic_num_for_key(
                                    long_term_key,
                                    NativeVulkanH264PictureFieldKind::from_slice(slice),
                                ))
                                .ok()
                            }),
                            self.long_term_references.get(&long_term_key),
                        ),
                    };
                    reference.map(|reference| NativeVulkanH264DecodeReferenceSnapshot {
                        frame_num: reference.frame_num,
                        field_pic_flag: reference.field_kind.field_pic_flag(),
                        bottom_field_flag: reference.field_kind.bottom_field_flag(),
                        used_for_long_term_reference,
                        long_term_frame_idx,
                        long_term_pic_num,
                        non_existing: reference.non_existing,
                        pic_order_cnt_val: reference.pic_order_cnt_val,
                        pic_order_cnt: reference.pic_order_cnt,
                        available: reference.dpb_slot != planned_output_slot,
                        source_access_unit_index: reference.source_access_unit_index,
                        dpb_slot: Some(reference.dpb_slot),
                    })
                })
                .collect::<NativeVulkanH264DecodeReferences>()
        } else {
            NativeVulkanH264DecodeReferences::new()
        };
        let available_reference_count = references
            .iter()
            .filter(|reference| reference.available)
            .count() as u32;
        let missing_reference_count =
            (references.len() as u32).saturating_sub(available_reference_count);
        let ready_for_decode_submit = current_frame_num.is_some()
            && current_pic_order_cnt_val.is_some()
            && current_pic_order_cnt.is_some()
            && unsupported_reason.is_none()
            && missing_reference_count == 0;

        let current_reference_long_term_frame_idx = adaptive_marking_plan
            .current_long_term_frame_idx
            .or(current_long_term_frame_idx);
        let mut dropped_reference_frame_nums = Vec::<u16>::new();
        let mut dropped_long_term_frame_indices = Vec::<u16>::new();
        let mut long_term_reference_conversions =
            Vec::<NativeVulkanH264LongTermReferenceConversionSnapshot>::new();
        let mut dropped_reference_slots = Vec::<u32>::new();
        if ready_for_decode_submit {
            if let Some(evicted_key) = evicted_key {
                match evicted_key {
                    NativeVulkanH264DpbSlotKey::ShortTerm(key) => {
                        self.remove_short_term_reference(key);
                    }
                    NativeVulkanH264DpbSlotKey::LongTerm(long_term_key) => {
                        self.remove_long_term_reference(long_term_key);
                    }
                }
            }
            self.clear_slot(planned_output_slot);
            for key in adaptive_marking_plan.drop_short_term_keys {
                if let Some(slot) = self.remove_short_term_reference(key) {
                    dropped_reference_frame_nums.push(key.frame_num);
                    dropped_reference_slots.push(slot);
                }
            }
            for long_term_key in adaptive_marking_plan.drop_long_term_keys {
                if let Some(slot) = self.remove_long_term_reference(long_term_key) {
                    if !dropped_long_term_frame_indices.contains(&long_term_key.frame_idx) {
                        dropped_long_term_frame_indices.push(long_term_key.frame_idx);
                    }
                    dropped_reference_slots.push(slot);
                }
            }
            for (key, long_term_frame_idx) in adaptive_marking_plan.convert_short_term_to_long_term
            {
                if let Some((slot, replaced_slot)) =
                    self.convert_short_term_to_long_term(key, long_term_frame_idx)
                {
                    long_term_reference_conversions.push(
                        NativeVulkanH264LongTermReferenceConversionSnapshot {
                            frame_num: key.frame_num,
                            long_term_frame_idx,
                            dpb_slot: slot,
                        },
                    );
                    if let Some(replaced_slot) = replaced_slot {
                        if !dropped_long_term_frame_indices.contains(&long_term_frame_idx) {
                            dropped_long_term_frame_indices.push(long_term_frame_idx);
                        }
                        if !dropped_reference_slots.contains(&replaced_slot) {
                            dropped_reference_slots.push(replaced_slot);
                        }
                    }
                }
            }
            if let (
                Some(slice),
                Some(current_frame_num),
                Some(current_pic_order_cnt_val),
                Some(current_pic_order_cnt),
            ) = (
                first_slice,
                current_frame_num,
                current_pic_order_cnt_val,
                current_pic_order_cnt,
            ) && slice.is_reference
            {
                let current_short_term_key = NativeVulkanH264ShortTermPictureKey::from_slice(slice);
                let reference_state = NativeVulkanH264DpbReferenceState {
                    source_access_unit_index: Some(access_unit.index),
                    dpb_slot: planned_output_slot,
                    pic_order_cnt_val: current_pic_order_cnt_val,
                    pic_order_cnt: current_pic_order_cnt,
                    frame_num: current_frame_num,
                    field_kind: current_short_term_key.field_kind,
                    non_existing: false,
                };
                if let Some(long_term_frame_idx) = current_reference_long_term_frame_idx {
                    let long_term_key =
                        NativeVulkanH264LongTermPictureKey::from_slice(slice, long_term_frame_idx);
                    if let Some(replaced_slot) = self.remove_long_term_reference(long_term_key) {
                        if replaced_slot != planned_output_slot {
                            dropped_reference_slots.push(replaced_slot);
                        }
                        if !dropped_long_term_frame_indices.contains(&long_term_frame_idx) {
                            dropped_long_term_frame_indices.push(long_term_frame_idx);
                        }
                    }
                    self.long_term_references
                        .insert(long_term_key, reference_state);
                    if let Some(slot) = self
                        .slot_to_reference_key
                        .get_mut(planned_output_slot as usize)
                    {
                        *slot = Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key));
                    }
                } else {
                    if let Some(slot) = self
                        .slot_to_reference_key
                        .get_mut(planned_output_slot as usize)
                    {
                        *slot = Some(NativeVulkanH264DpbSlotKey::ShortTerm(
                            current_short_term_key,
                        ));
                    }
                    self.short_term_references
                        .insert(current_short_term_key, reference_state);
                    self.short_term_reference_order
                        .retain(|key| *key != current_short_term_key);
                    self.short_term_reference_order.push(current_short_term_key);
                    for (old_frame_num, old_slot) in self.enforce_sliding_reference_window() {
                        dropped_reference_frame_nums.push(old_frame_num);
                        dropped_reference_slots.push(old_slot);
                    }
                }
                self.previous_reference_frame_num = Some(current_frame_num);
            }
        }

        NativeVulkanH264DecodeReferencePlanEntrySnapshot {
            access_unit_index: access_unit.index,
            pts_ms: access_unit.pts_ms,
            nal_type_label: first_slice.map(|slice| slice.nal_type_label),
            current_frame_num,
            current_pic_order_cnt_val,
            current_pic_order_cnt,
            current_long_term_frame_idx: current_reference_long_term_frame_idx,
            planned_output_slot,
            setup_slot_index: first_slice
                .filter(|slice| slice.is_reference)
                .map(|_| planned_output_slot as i32),
            evicted_frame_num,
            evicted_long_term_frame_idx,
            dropped_reference_frame_nums,
            dropped_long_term_frame_indices,
            inferred_non_existing_frame_nums: inferred_non_existing_plan.frame_nums,
            inferred_non_existing_references: inferred_non_existing_plan.references,
            inferred_dropped_reference_frame_nums: inferred_non_existing_plan
                .dropped_short_term_frame_nums,
            inferred_dropped_long_term_frame_indices: inferred_non_existing_plan
                .dropped_long_term_frame_indices,
            inferred_dropped_reference_slots: inferred_non_existing_plan.dropped_reference_slots,
            long_term_reference_conversions,
            dropped_reference_slots,
            requested_reference_count,
            references,
            available_reference_count,
            missing_reference_count,
            unsupported_reason,
            ready_for_decode_submit,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_decode_reference_plan(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
) -> Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot> {
    native_vulkan_h264_decode_reference_plan_with_gaps(
        access_units,
        dpb_slots,
        max_short_term_references,
        max_frame_num,
        true,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_decode_reference_plan_with_gaps(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
    gaps_in_frame_num_allowed: bool,
) -> Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanH264DecodeReferencePlanner::new(
        dpb_slots,
        max_short_term_references,
        max_frame_num,
        gaps_in_frame_num_allowed,
    );
    let mut plan = Vec::with_capacity(access_units.len());

    for access_unit in access_units {
        plan.push(planner.plan_next(access_unit));
    }

    plan
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_access_units_max_active_references(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
) -> u32 {
    access_units
        .iter()
        .filter_map(|access_unit| access_unit.first_slice.as_ref())
        .map(|slice| {
            let l0 = (slice.is_p || slice.is_b)
                .then(|| slice.num_ref_idx_l0_active_minus1.map(|value| value + 1))
                .flatten()
                .unwrap_or(0);
            let l1 = slice
                .is_b
                .then(|| slice.num_ref_idx_l1_active_minus1.map(|value| value + 1))
                .flatten()
                .unwrap_or(0);
            l0.saturating_add(l1)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_sps_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
    pub(in crate::renderer::native_vulkan) max_frame_num: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_streaming_bootstrap_scan_limit(
    capacity: usize,
) -> usize {
    std::env::var("GILDER_VULKAN_STREAMING_BOOTSTRAP_SCAN_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| capacity.max(1).saturating_mul(128).max(4096))
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_streaming_queue_has_field_pictures(
    queue: &NativeVulkanH264StreamingPacketQueue,
) -> bool {
    queue.bootstrap_access_units().iter().any(|access_unit| {
        access_unit
            .first_slice
            .as_ref()
            .is_some_and(|slice| slice.field_pic_flag)
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_picture_layout_candidates(
    sps: &NativeVulkanH264SpsSnapshot,
    stream_has_field_pictures: bool,
) -> Vec<vk::VideoDecodeH264PictureLayoutFlagsKHR> {
    if sps.frame_mbs_only_flag {
        return vec![vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE];
    }
    if stream_has_field_pictures {
        return vec![
            vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
            vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
        ];
    }
    vec![
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
    ]
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_access_unit_starts_recovery(
    access_unit: &NativeVulkanH264AccessUnitSnapshot,
) -> bool {
    access_unit
        .first_slice
        .as_ref()
        .is_some_and(|slice| slice.idr)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_first_recovery_access_unit_offset(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
) -> Option<usize> {
    access_units
        .iter()
        .position(native_vulkan_h264_access_unit_starts_recovery)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_streaming_dpb_slot_budget(
    sps_dpb_slots: u32,
    active_reference_pictures: u32,
) -> u32 {
    sps_dpb_slots
        .max(active_reference_pictures.saturating_add(1))
        .max(1)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_align_streaming_bootstrap(
    queue: &mut NativeVulkanH264StreamingPacketQueue,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_access_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_access_units = queue.bootstrap_access_units();
        if bootstrap_access_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap could not find a decodable AU window after skipping {} leading AU(s)",
                skipped_access_unit_indices.len()
            )));
        }
        let stream_sps_dpb_slots = native_vulkan_h264_sps_dpb_slot_count(&parameter_sets.sps);
        let max_frame_num = native_vulkan_h264_sps_max_frame_num(&parameter_sets.sps);
        let stream_max_active_reference_pictures = parameter_sets
            .sps
            .max_num_ref_frames
            .max(native_vulkan_h264_access_units_max_active_references(
                &bootstrap_access_units,
            ))
            .max(1);
        let stream_max_dpb_slots = native_vulkan_h264_streaming_dpb_slot_budget(
            stream_sps_dpb_slots,
            stream_max_active_reference_pictures,
        );
        let (window_dpb_slots, bootstrap_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
                &bootstrap_access_units,
                stream_max_dpb_slots,
                stream_max_active_reference_pictures,
                max_frame_num,
                parameter_sets.sps.gaps_in_frame_num_value_allowed_flag,
            );
        let stream_dpb_slots = window_dpb_slots.max(stream_sps_dpb_slots);
        let recovery_offset =
            native_vulkan_h264_first_recovery_access_unit_offset(&bootstrap_access_units);
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !entry.ready_for_decode_submit)
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanH264StreamingBootstrap {
                    stream_sps_dpb_slots,
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                    max_frame_num,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "H.264 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery AU after skipping {} leading AU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "H.264 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a recovery AU",
                        queue.bootstrap_discarded_access_units
                    )));
                };
                skipped_access_unit_indices.push(dropped.access_unit_index);
            }
            continue;
        };
        let first_unready = &bootstrap_plan[first_unready_offset];
        let discard_count = recovery_offset
            .filter(|offset| *offset > 0)
            .unwrap_or(usize::from(first_unready_offset == 0));
        if discard_count == 0 {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap AU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading AU(s): {}",
                first_unready.access_unit_index,
                queue.bootstrap_discarded_access_units,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        if usize::try_from(queue.bootstrap_discarded_access_units)
            .unwrap_or(usize::MAX)
            .saturating_add(discard_count)
            > scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable AU window; last leading AU {} was not decodable: {}",
                first_unready.access_unit_index,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "H.264 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_access_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_min_decodable_dpb_plan(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
) -> (u32, Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot>) {
    native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
        access_units,
        max_dpb_slots,
        max_short_term_references,
        max_frame_num,
        true,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
    gaps_in_frame_num_allowed: bool,
) -> (u32, Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_h264_decode_reference_plan_with_gaps(
            access_units,
            dpb_slots,
            max_short_term_references,
            max_frame_num,
            gaps_in_frame_num_allowed,
        );
        if plan.iter().all(|entry| entry.ready_for_decode_submit) {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}
include!("h264_h265_planner/h265_reference.rs");
