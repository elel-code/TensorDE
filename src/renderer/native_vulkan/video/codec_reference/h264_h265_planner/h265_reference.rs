
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) max_pic_order_cnt_lsb: u32,
    pub(in crate::renderer::native_vulkan) poc_to_decoded_slot: BTreeMap<i32, (u32, u32)>,
    pub(in crate::renderer::native_vulkan) slot_to_poc: Vec<Option<i32>>,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
    pub(in crate::renderer::native_vulkan) prev_poc_lsb: Option<i32>,
    pub(in crate::renderer::native_vulkan) prev_poc_msb: i32,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265ReferenceRequest {
    pub(in crate::renderer::native_vulkan) delta_poc: i32,
    pub(in crate::renderer::native_vulkan) poc: i32,
    pub(in crate::renderer::native_vulkan) used_for_long_term_reference: bool,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH265ReferenceRequest {
    const fn empty() -> Self {
        Self {
            delta_poc: 0,
            poc: 0,
            used_for_long_term_reference: false,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
const NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS: usize = 16;

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265ActiveDpbReference {
    pub(in crate::renderer::native_vulkan) poc: i32,
    pub(in crate::renderer::native_vulkan) used_for_long_term_reference: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265BeginSlotPolicy {
    pub(in crate::renderer::native_vulkan) active_only: bool,
    pub(in crate::renderer::native_vulkan) include_setup_slot: bool,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_begin_slot_policy_from_env()
-> NativeVulkanH265BeginSlotPolicy {
    NativeVulkanH265BeginSlotPolicy {
        active_only: matches!(
            std::env::var("GILDER_VULKAN_H265_BEGIN_REFERENCE_SLOTS")
                .ok()
                .as_deref(),
            Some("active-only") | Some("active")
        ),
        include_setup_slot: matches!(
            std::env::var("GILDER_VULKAN_H265_BEGIN_SETUP_SLOT")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("yes") | Some("begin")
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_st_curr_before(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "StCurrBefore",
        |reference| !reference.used_for_long_term_reference && reference.delta_poc < 0,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_st_curr_after(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "StCurrAfter",
        |reference| !reference.used_for_long_term_reference && reference.delta_poc > 0,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_lt_curr(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "LtCurr",
        |reference| reference.used_for_long_term_reference,
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h265_ref_pic_set_slots_by(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
    label: &'static str,
    include: fn(&NativeVulkanH265DecodeReferenceSnapshot) -> bool,
) -> Result<[u8; 8], NativeVulkanError> {
    let mut slots = [0xffu8; 8];
    let mut reference_count = 0usize;
    for reference in available_references
        .iter()
        .copied()
        .filter(|reference| include(reference))
    {
        if reference_count >= slots.len() {
            return Err(NativeVulkanError::Video(format!(
                "H.265 AU {access_unit_index} has {} {label} references; Vulkan STD H.265 decode supports at most 8 entries",
                reference_count + 1
            )));
        }
        let dpb_slot = reference.dpb_slot.ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 AU {access_unit_index} reference POC {} has no DPB slot",
                reference.poc
            ))
        })?;
        slots[reference_count] = native_vulkan_h265_u8(dpb_slot, "RefPicSet slotIndex")
            .map_err(NativeVulkanError::Video)?;
        reference_count += 1;
    }
    Ok(slots)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_begin_slot_refs(
    active_dpb_refs: &[Option<NativeVulkanH265ActiveDpbReference>],
    references: &[NativeVulkanH265DecodeReferenceSnapshot],
    reset_before_decode: bool,
    policy: NativeVulkanH265BeginSlotPolicy,
) -> Vec<(u32, Option<NativeVulkanH265ActiveDpbReference>)> {
    active_dpb_refs
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(slot, active_reference)| {
            let reference_override = references
                .iter()
                .find(|reference| reference.available && reference.dpb_slot == Some(slot as u32))
                .map(|reference| NativeVulkanH265ActiveDpbReference {
                    poc: reference.poc,
                    used_for_long_term_reference: reference.used_for_long_term_reference,
                });
            let had_active_reference = active_reference.is_some() || reference_override.is_some();
            if policy.active_only && !had_active_reference {
                return None;
            }
            let reference = if reset_before_decode {
                None
            } else {
                reference_override.or(active_reference)
            };
            Some((slot as u32, reference))
        })
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_apply_reference_usage(
    active_dpb_refs: &mut [Option<NativeVulkanH265ActiveDpbReference>],
    references: &[NativeVulkanH265DecodeReferenceSnapshot],
) {
    for reference in references.iter().filter(|reference| reference.available) {
        let Some(dpb_slot) = reference.dpb_slot else {
            continue;
        };
        let Some(slot) = active_dpb_refs.get_mut(dpb_slot as usize) else {
            continue;
        };
        *slot = Some(NativeVulkanH265ActiveDpbReference {
            poc: reference.poc,
            used_for_long_term_reference: reference.used_for_long_term_reference,
        });
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH265DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(
        dpb_slots: u32,
        max_pic_order_cnt_lsb: u32,
    ) -> Self {
        let dpb_slots = dpb_slots.max(1);
        Self {
            dpb_slots,
            max_pic_order_cnt_lsb: max_pic_order_cnt_lsb.max(1),
            poc_to_decoded_slot: BTreeMap::new(),
            slot_to_poc: vec![None; dpb_slots as usize],
            next_output_slot: 0,
            prev_poc_lsb: None,
            prev_poc_msb: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn reset_for_idr(&mut self) {
        self.poc_to_decoded_slot.clear();
        self.slot_to_poc.fill(None);
        self.next_output_slot = 0;
        self.prev_poc_lsb = Some(0);
        self.prev_poc_msb = 0;
    }

    pub(in crate::renderer::native_vulkan) fn choose_output_slot(
        &mut self,
        protected_pocs: &[i32],
    ) -> u32 {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if self
                .slot_to_poc
                .get(slot as usize)
                .is_none_or(Option::is_none)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            let slot_poc = self.slot_to_poc.get(slot as usize).copied().flatten();
            if slot_poc.is_none_or(|poc| !protected_pocs.contains(&poc)) {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        let slot = self.next_output_slot % self.dpb_slots;
        self.next_output_slot = (slot + 1) % self.dpb_slots;
        slot
    }

    pub(in crate::renderer::native_vulkan) fn derive_current_poc(
        &mut self,
        slice: &NativeVulkanH265AccessUnitSliceSnapshot,
    ) -> Option<i32> {
        if slice.idr {
            self.prev_poc_lsb = Some(0);
            self.prev_poc_msb = 0;
            return Some(0);
        }
        let poc_lsb = slice.pic_order_cnt_lsb? as i32;
        let max_lsb = i32::try_from(self.max_pic_order_cnt_lsb).unwrap_or(i32::MAX);
        let prev_lsb = self.prev_poc_lsb.unwrap_or(0);
        let prev_msb = self.prev_poc_msb;
        let half_max_lsb = max_lsb / 2;
        let poc_msb = if poc_lsb < prev_lsb && prev_lsb.saturating_sub(poc_lsb) >= half_max_lsb {
            prev_msb.saturating_add(max_lsb)
        } else if poc_lsb > prev_lsb && poc_lsb.saturating_sub(prev_lsb) > half_max_lsb {
            prev_msb.saturating_sub(max_lsb)
        } else {
            prev_msb
        };
        self.prev_poc_lsb = Some(poc_lsb);
        self.prev_poc_msb = poc_msb;
        Some(poc_msb.saturating_add(poc_lsb))
    }

    pub(in crate::renderer::native_vulkan) fn derive_long_term_reference_poc(
        &self,
        slice: &NativeVulkanH265AccessUnitSliceSnapshot,
        current_poc: i32,
        reference: &NativeVulkanH265LongTermReferenceSnapshot,
    ) -> Option<i32> {
        if !reference.used_by_current {
            return None;
        }
        let max_lsb = i32::try_from(self.max_pic_order_cnt_lsb.max(1)).unwrap_or(i32::MAX);
        let poc_lsb = i32::try_from(reference.poc_lsb).ok()?;
        if let Some(delta_poc_msb_cycle_lt) = reference.delta_poc_msb_cycle_lt {
            let current_poc_lsb = slice.pic_order_cnt_lsb? as i32;
            let delta_msb = i32::try_from(delta_poc_msb_cycle_lt).ok()?;
            return Some(
                current_poc
                    .saturating_sub(delta_msb.saturating_mul(max_lsb))
                    .saturating_sub(current_poc_lsb.saturating_sub(poc_lsb)),
            );
        }
        self.poc_to_decoded_slot
            .keys()
            .copied()
            .find(|decoded_poc| decoded_poc.rem_euclid(max_lsb) == poc_lsb.rem_euclid(max_lsb))
            .or(Some(poc_lsb))
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        access_unit: &NativeVulkanH265AccessUnitSnapshot,
    ) -> NativeVulkanH265DecodeReferencePlanEntrySnapshot {
        let first_slice = access_unit.first_slice.as_ref();
        let idr = first_slice.is_some_and(|slice| slice.idr);
        if idr {
            self.reset_for_idr();
        }
        let current_poc = first_slice.and_then(|slice| self.derive_current_poc(slice));
        let mut unsupported_reason = access_unit.first_slice_parse_error.clone();
        let mut reference_requests =
            [NativeVulkanH265ReferenceRequest::empty(); NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS];
        let mut reference_request_count = 0usize;
        if let (Some(slice), Some(current_poc)) = (first_slice, current_poc) {
            for delta_poc in slice.short_term_reference_delta_pocs.iter().copied() {
                if let Some(request) = reference_requests.get_mut(reference_request_count) {
                    *request = NativeVulkanH265ReferenceRequest {
                        delta_poc,
                        poc: current_poc.saturating_add(delta_poc),
                        used_for_long_term_reference: false,
                    };
                    reference_request_count += 1;
                } else if unsupported_reason.is_none() {
                    unsupported_reason = Some(format!(
                        "H.265 slice requests more than FFmpeg HEVC_MAX_REFS ({NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS}) active references"
                    ));
                }
            }
            for long_term_reference in &slice.long_term_references {
                if let Some(poc) =
                    self.derive_long_term_reference_poc(slice, current_poc, long_term_reference)
                {
                    if let Some(request) = reference_requests.get_mut(reference_request_count) {
                        *request = NativeVulkanH265ReferenceRequest {
                            delta_poc: poc.saturating_sub(current_poc),
                            poc,
                            used_for_long_term_reference: true,
                        };
                        reference_request_count += 1;
                    } else if unsupported_reason.is_none() {
                        unsupported_reason = Some(format!(
                            "H.265 slice requests more than FFmpeg HEVC_MAX_REFS ({NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS}) active references"
                        ));
                    }
                }
            }
        }
        let reference_requests = &reference_requests[..reference_request_count];
        let mut protected_pocs = [0i32; NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS];
        for (index, request) in reference_requests.iter().enumerate() {
            protected_pocs[index] = request.poc;
        }
        let planned_output_slot = if current_poc.is_some() {
            self.choose_output_slot(&protected_pocs[..reference_requests.len()])
        } else {
            self.next_output_slot % self.dpb_slots
        };
        let evicted_poc = self
            .slot_to_poc
            .get(planned_output_slot as usize)
            .copied()
            .flatten();
        let mut references = NativeVulkanH265DecodeReferences::new();
        for request in reference_requests.iter().copied() {
            let source = self.poc_to_decoded_slot.get(&request.poc).copied();
            let available = source.is_some_and(|(_, slot)| slot != planned_output_slot);
            references.push(NativeVulkanH265DecodeReferenceSnapshot {
                delta_poc: request.delta_poc,
                poc: request.poc,
                used_for_long_term_reference: request.used_for_long_term_reference,
                available,
                source_access_unit_index: source.map(|(index, _)| index),
                dpb_slot: source.map(|(_, slot)| slot),
            });
        }
        let mut missing_reference_pocs = Vec::with_capacity(reference_requests.len());
        for reference in references.iter().filter(|reference| !reference.available) {
            missing_reference_pocs.push(reference.poc);
        }
        let available_reference_count = references
            .iter()
            .filter(|reference| reference.available)
            .count() as u32;
        let missing_reference_count = missing_reference_pocs.len() as u32;
        let ready_for_decode_submit =
            current_poc.is_some() && unsupported_reason.is_none() && missing_reference_count == 0;

        if ready_for_decode_submit && let Some(current_poc) = current_poc {
            if let Some(evicted_poc) = evicted_poc {
                self.poc_to_decoded_slot.remove(&evicted_poc);
            }
            if let Some(slot) = self.slot_to_poc.get_mut(planned_output_slot as usize) {
                *slot = Some(current_poc);
            }
            self.poc_to_decoded_slot
                .insert(current_poc, (access_unit.index, planned_output_slot));
        }

        NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: access_unit.index,
            pts_ms: access_unit.pts_ms,
            nal_type_label: first_slice.map(|slice| slice.nal_type_label),
            current_poc,
            planned_output_slot,
            setup_slot_index: current_poc.map(|_| planned_output_slot as i32),
            evicted_poc,
            references,
            available_reference_count,
            missing_reference_count,
            missing_reference_pocs,
            unsupported_reason,
            ready_for_decode_submit,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_decode_reference_plan(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
    dpb_slots: u32,
    max_pic_order_cnt_lsb: u32,
) -> Vec<NativeVulkanH265DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanH265DecodeReferencePlanner::new(dpb_slots, max_pic_order_cnt_lsb);
    let mut plan = Vec::with_capacity(access_units.len());

    for access_unit in access_units {
        plan.push(planner.plan_next(access_unit));
    }

    plan
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_min_decodable_dpb_plan(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_pic_order_cnt_lsb: u32,
) -> (u32, Vec<NativeVulkanH265DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_h265_decode_reference_plan(
            access_units,
            dpb_slots,
            max_pic_order_cnt_lsb,
        );
        if plan.iter().all(|entry| entry.ready_for_decode_submit) {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_access_unit_starts_recovery(
    access_unit: &NativeVulkanH265AccessUnitSnapshot,
) -> bool {
    access_unit
        .first_slice
        .as_ref()
        .is_some_and(|slice| slice.idr)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_sps_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
    pub(in crate::renderer::native_vulkan) stream_max_pic_order_cnt_lsb: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_align_streaming_bootstrap(
    queue: &mut NativeVulkanH265StreamingPacketQueue,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_access_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_access_units = queue.bootstrap_access_units();
        if bootstrap_access_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "H.265 streaming bootstrap could not find a decodable AU window after skipping {} leading AU(s)",
                skipped_access_unit_indices.len()
            )));
        }
        let stream_sps_dpb_slots = native_vulkan_h265_sps_dpb_slot_count(&parameter_sets.sps);
        let stream_max_pic_order_cnt_lsb =
            native_vulkan_h265_sps_max_pic_order_cnt_lsb(&parameter_sets.sps);
        let stream_max_active_reference_pictures =
            native_vulkan_h265_access_units_max_active_references(&bootstrap_access_units)
                .max(stream_sps_dpb_slots.saturating_sub(1))
                .max(1);
        let (window_dpb_slots, bootstrap_plan) = native_vulkan_h265_min_decodable_dpb_plan(
            &bootstrap_access_units,
            stream_sps_dpb_slots,
            stream_max_pic_order_cnt_lsb,
        );
        let stream_dpb_slots = window_dpb_slots.max(stream_sps_dpb_slots);
        let recovery_offset = bootstrap_access_units
            .iter()
            .position(native_vulkan_h265_access_unit_starts_recovery);
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !entry.ready_for_decode_submit)
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanH265StreamingBootstrap {
                    stream_sps_dpb_slots,
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                    stream_max_pic_order_cnt_lsb,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "H.265 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery AU after skipping {} leading AU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "H.265 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a recovery AU",
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
                "H.265 streaming bootstrap AU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading AU(s); missing POCs {:?}",
                first_unready.access_unit_index,
                queue.bootstrap_discarded_access_units,
                first_unready.missing_reference_pocs
            )));
        }
        if usize::try_from(queue.bootstrap_discarded_access_units)
            .unwrap_or(usize::MAX)
            .saturating_add(discard_count)
            > scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "H.265 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable AU window; last leading AU {} was missing POCs {:?}",
                first_unready.access_unit_index, first_unready.missing_reference_pocs
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "H.265 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_access_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_access_units_max_active_references(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
) -> u32 {
    access_units
        .iter()
        .filter_map(|access_unit| access_unit.first_slice.as_ref())
        .map(|slice| {
            let short_term_count = slice
                .short_term_reference_delta_pocs
                .len()
                .min(u32::MAX as usize) as u32;
            let long_term_count = slice
                .long_term_references
                .iter()
                .filter(|reference| reference.used_by_current)
                .count()
                .min(u32::MAX as usize) as u32;
            short_term_count.saturating_add(long_term_count)
        })
        .max()
        .unwrap_or(0)
}
