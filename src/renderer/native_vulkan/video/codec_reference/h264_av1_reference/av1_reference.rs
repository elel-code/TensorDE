#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1ReferenceMapEntry {
    pub(in crate::renderer::native_vulkan) slot: u32,
    pub(in crate::renderer::native_vulkan) order_hint: Option<u8>,
    pub(in crate::renderer::native_vulkan) frame_type: u8,
    pub(in crate::renderer::native_vulkan) frame_width: Option<u32>,
    pub(in crate::renderer::native_vulkan) frame_height: Option<u32>,
    pub(in crate::renderer::native_vulkan) render_width: Option<u32>,
    pub(in crate::renderer::native_vulkan) render_height: Option<u32>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
    pub(in crate::renderer::native_vulkan) reference_map:
        [Option<NativeVulkanAv1ReferenceMapEntry>; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(dpb_slots: u32) -> Self {
        Self {
            dpb_slots: dpb_slots.max(1),
            next_output_slot: 0,
            reference_map: [None; 8],
        }
    }

    pub(in crate::renderer::native_vulkan) fn map_slot_indices(&self) -> Vec<i32> {
        self.reference_map
            .iter()
            .map(|entry| entry.map(|entry| entry.slot as i32).unwrap_or(-1))
            .collect()
    }

    pub(in crate::renderer::native_vulkan) fn map_order_hints(&self) -> Vec<Option<u8>> {
        self.reference_map
            .iter()
            .map(|entry| entry.and_then(|entry| entry.order_hint))
            .collect()
    }

    pub(in crate::renderer::native_vulkan) fn reference_name_order_hints(
        &self,
        ref_frame_indices: &[i8],
    ) -> Vec<Option<u8>> {
        let mut order_hints = vec![None; 8];
        for (reference_name_minus_one, ref_idx) in ref_frame_indices.iter().take(7).enumerate() {
            if !(0..=7).contains(ref_idx) {
                continue;
            }
            order_hints[reference_name_minus_one + 1] =
                self.reference_map[*ref_idx as usize].and_then(|entry| entry.order_hint);
        }
        order_hints
    }

    pub(in crate::renderer::native_vulkan) fn allocate_output_slot(
        &mut self,
        protected_slots: &[u32],
    ) -> Option<u32> {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if protected_slots.contains(&slot) {
                continue;
            }
            if !self
                .reference_map
                .iter()
                .flatten()
                .any(|entry| entry.slot == slot)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return Some(slot);
            }
        }
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if !protected_slots.contains(&slot) {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return Some(slot);
            }
        }
        None
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        temporal_unit: &NativeVulkanAv1TemporalUnitSnapshot,
    ) -> NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
        let reference_name_slot_indices = self.map_slot_indices();
        let map_order_hints = self.map_order_hints();
        let Some(submit) = temporal_unit.first_frame_submit.as_ref() else {
            return NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
                temporal_unit_index: temporal_unit.index,
                frame_type_label: "none",
                show_existing_frame: false,
                frame_to_show_map_idx: None,
                show_frame: false,
                order_hint: None,
                current_frame_id: None,
                expected_frame_ids: Vec::new(),
                refresh_frame_flags: 0,
                output_slot: None,
                displayed_slot: None,
                reference_name_slot_indices,
                reference_name_order_hints: vec![None; 8],
                map_order_hints,
                ref_frame_indices: Vec::new(),
                decode_reference_slots: Vec::new(),
                refreshed_reference_names: Vec::new(),
                missing_reference_names: Vec::new(),
                missing_reference_count: 0,
                references_resolved: false,
                submit_fields_ready: false,
                ready_for_decode_submit: false,
                ready_for_display_handoff: false,
                unsupported_reason: Some("AV1 temporal unit has no parsed frame header".to_owned()),
                map_slot_indices_after: self.map_slot_indices(),
                map_order_hints_after: self.map_order_hints(),
            };
        };
        let reference_name_order_hints = self.reference_name_order_hints(&submit.ref_frame_indices);

        if submit.show_existing_frame {
            let map_idx = submit.frame_to_show_map_idx;
            let displayed_entry = map_idx
                .and_then(|index| self.reference_map.get(index as usize))
                .and_then(|entry| *entry);
            let displayed_slot = displayed_entry.map(|entry| entry.slot);
            let missing_reference_names = if displayed_slot.is_some() {
                Vec::new()
            } else {
                map_idx.into_iter().collect()
            };
            let ready_for_display_handoff = displayed_slot.is_some();
            let inferred_frame_type = displayed_entry
                .map(|entry| entry.frame_type)
                .unwrap_or(submit.frame_type);
            let refresh_frame_flags = if ready_for_display_handoff && inferred_frame_type == 0 {
                0xff
            } else {
                submit.refresh_frame_flags
            };
            let refreshed_reference_names = (0..8)
                .filter(|index| (refresh_frame_flags & (1u8 << index)) != 0)
                .map(|index| index as u8)
                .collect::<Vec<_>>();
            if ready_for_display_handoff && let Some(displayed_entry) = displayed_entry {
                for index in &refreshed_reference_names {
                    self.reference_map[*index as usize] = Some(displayed_entry);
                }
            }
            return NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
                temporal_unit_index: temporal_unit.index,
                frame_type_label: native_vulkan_av1_frame_type_label(inferred_frame_type),
                show_existing_frame: true,
                frame_to_show_map_idx: submit.frame_to_show_map_idx,
                show_frame: submit.show_frame,
                order_hint: submit.order_hint,
                current_frame_id: submit.current_frame_id,
                expected_frame_ids: submit.expected_frame_ids.clone(),
                refresh_frame_flags,
                output_slot: None,
                displayed_slot,
                reference_name_slot_indices,
                reference_name_order_hints,
                map_order_hints,
                ref_frame_indices: submit.ref_frame_indices.clone(),
                decode_reference_slots: Vec::new(),
                refreshed_reference_names,
                missing_reference_count: missing_reference_names.len() as u32,
                missing_reference_names,
                references_resolved: ready_for_display_handoff,
                submit_fields_ready: false,
                ready_for_decode_submit: false,
                ready_for_display_handoff,
                unsupported_reason: if ready_for_display_handoff {
                    None
                } else {
                    Some("AV1 show_existing_frame references an unavailable map index".to_owned())
                },
                map_slot_indices_after: self.map_slot_indices(),
                map_order_hints_after: self.map_order_hints(),
            };
        }

        let mut decode_reference_slots = Vec::with_capacity(submit.ref_frame_indices.len());
        let mut missing_reference_names = Vec::new();
        for ref_idx in &submit.ref_frame_indices {
            if *ref_idx < 0 || *ref_idx > 7 {
                missing_reference_names.push(0xff);
                decode_reference_slots.push(-1);
                continue;
            }
            let map_idx = *ref_idx as usize;
            match self.reference_map[map_idx] {
                Some(entry) => decode_reference_slots.push(entry.slot as i32),
                None => {
                    missing_reference_names.push(*ref_idx as u8);
                    decode_reference_slots.push(-1);
                }
            }
        }
        let references_resolved = missing_reference_names.is_empty();
        let refreshed_reference_names = (0..8)
            .filter(|index| (submit.refresh_frame_flags & (1u8 << index)) != 0)
            .map(|index| index as u8)
            .collect::<Vec<_>>();
        let mut protected_slots = decode_reference_slots
            .iter()
            .filter_map(|slot| u32::try_from(*slot).ok())
            .collect::<Vec<_>>();
        for (index, entry) in self.reference_map.iter().enumerate() {
            if refreshed_reference_names.contains(&(index as u8)) {
                continue;
            }
            if let Some(entry) = entry
                && !protected_slots.contains(&entry.slot)
            {
                protected_slots.push(entry.slot);
            }
        }
        let output_slot = self.allocate_output_slot(&protected_slots);
        if let Some(output_slot_value) = output_slot {
            for index in &refreshed_reference_names {
                self.reference_map[*index as usize] = Some(NativeVulkanAv1ReferenceMapEntry {
                    slot: output_slot_value,
                    order_hint: submit.order_hint,
                    frame_type: submit.frame_type,
                    frame_width: submit.frame_width,
                    frame_height: submit.frame_height,
                    render_width: submit.render_width,
                    render_height: submit.render_height,
                });
            }
        }

        let submit_fields_ready = submit.vulkan_submit_candidate;
        let output_slot_available = output_slot.is_some();
        let ready_for_decode_submit =
            references_resolved && submit_fields_ready && output_slot_available;
        let unsupported_reason = if !references_resolved {
            Some(format!(
                "AV1 reference map is missing reference name(s) {:?}",
                missing_reference_names
            ))
        } else if !output_slot_available {
            Some(format!(
                "AV1 reference map has no free DPB output slot with {} slot(s); protected slots {:?}",
                self.dpb_slots, protected_slots
            ))
        } else if !submit_fields_ready {
            submit.unsupported_reason.clone().or_else(|| {
                Some("AV1 frame header is reference-ready but not submit-ready".to_owned())
            })
        } else {
            None
        };

        NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: temporal_unit.index,
            frame_type_label: submit.frame_type_label,
            show_existing_frame: false,
            frame_to_show_map_idx: submit.frame_to_show_map_idx,
            show_frame: submit.show_frame,
            order_hint: submit.order_hint,
            current_frame_id: submit.current_frame_id,
            expected_frame_ids: submit.expected_frame_ids.clone(),
            refresh_frame_flags: submit.refresh_frame_flags,
            output_slot,
            displayed_slot: if submit.show_frame { output_slot } else { None },
            reference_name_slot_indices,
            reference_name_order_hints,
            map_order_hints,
            ref_frame_indices: submit.ref_frame_indices.clone(),
            decode_reference_slots,
            refreshed_reference_names,
            missing_reference_count: missing_reference_names.len() as u32,
            missing_reference_names,
            references_resolved,
            submit_fields_ready,
            ready_for_decode_submit,
            ready_for_display_handoff: false,
            unsupported_reason,
            map_slot_indices_after: self.map_slot_indices(),
            map_order_hints_after: self.map_order_hints(),
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_decode_reference_plan(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
    dpb_slots: u32,
) -> Vec<NativeVulkanAv1DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanAv1DecodeReferencePlanner::new(dpb_slots);
    temporal_units
        .iter()
        .map(|temporal_unit| planner.plan_next(temporal_unit))
        .collect()
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_min_decodable_dpb_plan(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
    max_dpb_slots: u32,
) -> (u32, Vec<NativeVulkanAv1DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_av1_decode_reference_plan(temporal_units, dpb_slots);
        if plan
            .iter()
            .all(|entry| entry.ready_for_decode_submit || entry.ready_for_display_handoff)
        {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_temporal_units_max_active_references(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
) -> u32 {
    temporal_units
        .iter()
        .filter_map(|temporal_unit| temporal_unit.first_frame_submit.as_ref())
        .map(|submit| {
            submit
                .ref_frame_indices
                .iter()
                .filter(|index| **index >= 0)
                .count()
                .min(u32::MAX as usize) as u32
        })
        .max()
        .unwrap_or(0)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_temporal_unit_starts_recovery(
    temporal_unit: &NativeVulkanAv1TemporalUnitSnapshot,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> bool {
    temporal_unit
        .first_frame_submit
        .as_ref()
        .is_some_and(|submit| {
            submit.frame_type == 0
                && submit.show_frame
                && submit.vulkan_submit_candidate
                && temporal_unit
                    .sequence_header
                    .as_ref()
                    .unwrap_or(sequence_header)
                    .vulkan_std_session_parameters_ready
        })
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_align_streaming_bootstrap(
    queue: &mut NativeVulkanAv1StreamingPacketQueue,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_temporal_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_temporal_units = queue.bootstrap_access_units();
        if bootstrap_temporal_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "AV1 streaming bootstrap could not find a decodable TU window after skipping {} leading TU(s)",
                skipped_temporal_unit_indices.len()
            )));
        }
        // AV1 has eight named reference slots. Real streams can also contain
        // showable non-reference frames (`refresh_frame_flags == 0`), which need
        // a transient output slot when DPB and output coincide.
        let stream_max_dpb_slots = 16;
        let stream_max_active_reference_pictures =
            native_vulkan_av1_temporal_units_max_active_references(&bootstrap_temporal_units)
                .max(7)
                .max(1);
        let (stream_dpb_slots_for_window, bootstrap_plan) =
            native_vulkan_av1_min_decodable_dpb_plan(
                &bootstrap_temporal_units,
                stream_max_dpb_slots,
            );
        let stream_dpb_slots = stream_dpb_slots_for_window.max(9).min(stream_max_dpb_slots);
        let recovery_offset = bootstrap_temporal_units.iter().position(|temporal_unit| {
            native_vulkan_av1_temporal_unit_starts_recovery(temporal_unit, sequence_header)
        });
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !(entry.ready_for_decode_submit || entry.ready_for_display_handoff))
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanAv1StreamingBootstrap {
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "AV1 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery TU after skipping {} leading TU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "AV1 streaming bootstrap reached EOS after skipping {} leading TU(s) without finding a recovery TU",
                        queue.bootstrap_discarded_access_units
                    )));
                };
                skipped_temporal_unit_indices.push(dropped.access_unit_index);
            }
            continue;
        };
        let first_unready = &bootstrap_plan[first_unready_offset];
        let discard_count = recovery_offset
            .filter(|offset| *offset > 0)
            .unwrap_or(usize::from(first_unready_offset == 0));
        if discard_count == 0 {
            return Err(NativeVulkanError::Video(format!(
                "AV1 streaming bootstrap TU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading TU(s): {}",
                first_unready.temporal_unit_index,
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
                "AV1 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable TU window; last leading TU {} was not decodable: {}",
                first_unready.temporal_unit_index,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "AV1 streaming bootstrap reached EOS after skipping {} leading TU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_temporal_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) max_short_term_references: u32,
    pub(in crate::renderer::native_vulkan) max_frame_num: u32,
    pub(in crate::renderer::native_vulkan) gaps_in_frame_num_allowed: bool,
    pub(in crate::renderer::native_vulkan) previous_reference_frame_num: Option<u16>,
    pub(in crate::renderer::native_vulkan) short_term_references:
        BTreeMap<NativeVulkanH264ShortTermPictureKey, NativeVulkanH264DpbReferenceState>,
    pub(in crate::renderer::native_vulkan) long_term_references:
        BTreeMap<NativeVulkanH264LongTermPictureKey, NativeVulkanH264DpbReferenceState>,
    pub(in crate::renderer::native_vulkan) slot_to_reference_key:
        Vec<Option<NativeVulkanH264DpbSlotKey>>,
    pub(in crate::renderer::native_vulkan) short_term_reference_order:
        Vec<NativeVulkanH264ShortTermPictureKey>,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
}
