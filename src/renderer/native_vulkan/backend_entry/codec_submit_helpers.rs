use super::*;

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanAv1BeginReferenceSlotStrategy {
    FullDpbGeneric,
    DecodeRefsAndSetup,
    DecodeRefsAndCurrentInactive,
    ActiveRefs,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanAv1BeginReferenceSlotStrategy {
    pub(super) fn from_env() -> Self {
        match std::env::var("GILDER_VULKAN_AV1_BEGIN_REFERENCE_SLOTS")
            .ok()
            .as_deref()
        {
            Some("decode-refs-setup") | Some("decode") | Some("sample") => Self::DecodeRefsAndSetup,
            Some("decode-refs-current-inactive") | Some("ffmpeg") | Some("current-inactive") => {
                Self::DecodeRefsAndCurrentInactive
            }
            Some("active") | Some("active-only") | Some("active-refs") => Self::ActiveRefs,
            Some("full-dpb") | Some("full-dpb-generic") => Self::FullDpbGeneric,
            _ => Self::DecodeRefsAndCurrentInactive,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FullDpbGeneric => "full-dpb-generic",
            Self::DecodeRefsAndSetup => "decode-refs-and-setup",
            Self::DecodeRefsAndCurrentInactive => "decode-refs-current-inactive",
            Self::ActiveRefs => "active-refs",
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_relative_dist_from_order_hint_bits(
    enable_order_hint: bool,
    order_hint_bits_minus_1: Option<u8>,
    a: u8,
    b: u8,
) -> i32 {
    if !enable_order_hint {
        return 0;
    }
    let bits = (u32::from(order_hint_bits_minus_1.unwrap_or(0)) + 1).clamp(1, 8);
    let mask = (1i32 << bits) - 1;
    let a = i32::from(a) & mask;
    let b = i32::from(b) & mask;
    let diff = a - b;
    let midpoint = 1i32 << (bits - 1);
    (diff & (midpoint - 1)) - (diff & midpoint)
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_relative_dist(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    a: u8,
    b: u8,
) -> i32 {
    native_vulkan_av1_relative_dist_from_order_hint_bits(
        sequence_header.enable_order_hint,
        sequence_header.order_hint_bits_minus_1,
        a,
        b,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    if !sequence_header.enable_order_hint {
        return 0;
    }
    let mut packed = 0u8;
    for ref_name in 1..8 {
        let relative = native_vulkan_av1_relative_dist(
            sequence_header,
            current_order_hint,
            order_hints[ref_name],
        );
        if relative < 0 {
            packed |= 1u8 << ref_name;
        }
    }
    packed
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_current_ref_frame_sign_bias(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    if matches!(frame_type, 0 | 2) {
        return 0;
    }
    native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
        sequence_header,
        current_order_hint,
        order_hints,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_dpb_reference_sign_bias(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    current_order_hint: u8,
    order_hints: [u8; 8],
) -> u8 {
    match std::env::var("GILDER_VULKAN_AV1_REFERENCE_SIGN_BIAS")
        .ok()
        .as_deref()
    {
        Some("zero") => 0,
        Some("all") | Some("all-frames") => native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
            sequence_header,
            current_order_hint,
            order_hints,
        ),
        _ => native_vulkan_av1_current_ref_frame_sign_bias(
            sequence_header,
            frame_type,
            current_order_hint,
            order_hints,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_setup_saved_order_hints(
    order_hints: [u8; 8],
    _refresh_frame_flags: u8,
    _current_order_hint: u8,
) -> [u8; 8] {
    order_hints
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_current_setup_saved_order_hints(
    _order_hints: [u8; 8],
    _refresh_frame_flags: u8,
    _current_order_hint: u8,
) -> [u8; 8] {
    [0; 8]
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_expected_frame_ids_array(expected_frame_ids: &[u32]) -> [u32; 8] {
    let mut values = [0u32; 8];
    for (index, value) in expected_frame_ids.iter().take(8).copied().enumerate() {
        values[index] = value;
    }
    values
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_order_hint_offset_enabled(_vendor_id: u32) -> bool {
    match std::env::var("GILDER_VULKAN_AV1_ORDER_HINT_OFFSET")
        .ok()
        .as_deref()
    {
        Some("off") | Some("false") | Some("0") | Some("none") | Some("standard") => false,
        Some("on") | Some("true") | Some("1") | Some("ffmpeg") | Some("nvidia")
        | Some("shift-left") => true,
        _ => false,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_std_order_hints(
    order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> [u8; 8] {
    if !order_hint_offset_enabled {
        return order_hints;
    }
    let mut shifted = [0u8; 8];
    shifted[..7].copy_from_slice(&order_hints[1..8]);
    shifted
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_order_hints_array(hints: &[Option<u8>]) -> [u8; 8] {
    let mut values = [0u8; 8];
    for (index, hint) in hints.iter().take(8).enumerate() {
        values[index] = hint.unwrap_or(0);
    }
    values
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_picture_order_hints_for_submit(
    reference_name_order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> [u8; 8] {
    native_vulkan_av1_std_order_hints(reference_name_order_hints, order_hint_offset_enabled)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanAv1ReferenceHistory {
    frame_width: u32,
    frame_height: u32,
    render_width: u32,
    render_height: u32,
    segmentation: NativeVulkanAv1ParsedSegmentation,
    loop_filter_ref_deltas: [i8; 8],
    loop_filter_mode_deltas: [i8; 2],
}

#[cfg(feature = "native-vulkan-video")]
impl From<NativeVulkanAv1ActiveDpbReference> for NativeVulkanAv1ReferenceHistory {
    pub(super) fn from(reference: NativeVulkanAv1ActiveDpbReference) -> Self {
        Self {
            frame_width: reference.frame_width,
            frame_height: reference.frame_height,
            render_width: reference.render_width,
            render_height: reference.render_height,
            segmentation: reference.segmentation,
            loop_filter_ref_deltas: reference.loop_filter_ref_deltas,
            loop_filter_mode_deltas: reference.loop_filter_mode_deltas,
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanAv1FrameHeaderReferenceContext {
    reference_name_order_hints: [u8; 8],
    reference_name_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    reference_histories:
        [Option<NativeVulkanAv1ReferenceHistory>; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1PreparedReferenceContext {
    reference_name_order_hints: [u8; 8],
    reference_name_dpb_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    reference_context: NativeVulkanAv1FrameHeaderReferenceContext,
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_prepared_reference_context(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    active_dpb_refs: &[Option<NativeVulkanAv1ActiveDpbReference>],
) -> NativeVulkanAv1PreparedReferenceContext {
    let reference_name_dpb_slot_indices = native_vulkan_av1_reference_name_slot_indices(entry);
    let reference_name_order_hints =
        native_vulkan_av1_order_hints_array(&entry.reference_name_order_hints);
    let mut reference_histories =
        [None::<NativeVulkanAv1ReferenceHistory>; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (reference_index, slot_index) in reference_name_dpb_slot_indices.iter().copied().enumerate()
    {
        let Ok(slot_index) = usize::try_from(slot_index) else {
            continue;
        };
        reference_histories[reference_index] = active_dpb_refs
            .get(slot_index)
            .and_then(|reference| reference.map(NativeVulkanAv1ReferenceHistory::from));
    }
    NativeVulkanAv1PreparedReferenceContext {
        reference_name_order_hints,
        reference_name_dpb_slot_indices,
        reference_context: NativeVulkanAv1FrameHeaderReferenceContext {
            reference_name_order_hints,
            reference_name_slot_indices: reference_name_dpb_slot_indices,
            reference_histories,
        },
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1FrameHeaderReferenceContext {
    pub(super) fn primary_reference_history(
        &self,
        primary_ref_frame: Option<u8>,
    ) -> Option<NativeVulkanAv1ReferenceHistory> {
        if native_vulkan_av1_primary_ref_none(primary_ref_frame) {
            return None;
        }
        let index = usize::from(primary_ref_frame?);
        self.reference_histories.get(index).copied().flatten()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_skip_mode_frame_from_order_hints(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
    frame_type: u8,
    error_resilient_mode: bool,
    reference_select: bool,
    current_order_hint: u8,
    reference_name_order_hints: [u8; 8],
    reference_name_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
) -> Option<[u8; 2]> {
    if !sequence_header.enable_order_hint
        || error_resilient_mode
        || frame_type != 1
        || !reference_select
    {
        return None;
    }

    let mut ref0 = None::<u8>;
    let mut ref1 = None::<u8>;
    let mut ref0_hint = None::<u8>;
    let mut ref1_hint = None::<u8>;

    for ref_name_minus_one in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
        if reference_name_slot_indices[ref_name_minus_one] < 0 {
            continue;
        }
        let ref_name = (ref_name_minus_one + 1) as u8;
        let ref_order_hint = reference_name_order_hints[ref_name as usize];
        let relative =
            native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, current_order_hint);
        if relative < 0
            && ref0_hint.is_none_or(|hint| {
                native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) > 0
            })
        {
            ref0 = Some(ref_name);
            ref0_hint = Some(ref_order_hint);
        }
        if relative > 0
            && ref1_hint.is_none_or(|hint| {
                native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) < 0
            })
        {
            ref1 = Some(ref_name);
            ref1_hint = Some(ref_order_hint);
        }
    }

    match (ref0, ref1) {
        (Some(left), Some(right)) => Some([left.min(right), left.max(right)]),
        (Some(left), None) => {
            let first_forward_hint = ref0_hint?;
            let mut second = None::<u8>;
            let mut second_hint = None::<u8>;
            for ref_name_minus_one in 0..vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR {
                if reference_name_slot_indices[ref_name_minus_one] < 0 {
                    continue;
                }
                let ref_name = (ref_name_minus_one + 1) as u8;
                let ref_order_hint = reference_name_order_hints[ref_name as usize];
                if native_vulkan_av1_relative_dist(
                    sequence_header,
                    ref_order_hint,
                    first_forward_hint,
                ) < 0
                    && second_hint.is_none_or(|hint| {
                        native_vulkan_av1_relative_dist(sequence_header, ref_order_hint, hint) > 0
                    })
                {
                    second = Some(ref_name);
                    second_hint = Some(ref_order_hint);
                }
            }
            let right = second?;
            Some([left.min(right), left.max(right)])
        }
        _ => None,
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_reference_name_slot_indices(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
) -> [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR] {
    let mut slots = [-1i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (index, slot) in entry
        .decode_reference_slots
        .iter()
        .take(vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR)
        .enumerate()
    {
        slots[index] = *slot;
    }
    slots
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_reference_name_decode_slot_indices(
    reference_name_dpb_slot_indices: [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR],
    unique_reference_slots: &[u32],
) -> [i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR] {
    let mut slots = [-1i32; vk::MAX_VIDEO_AV1_REFERENCES_PER_FRAME_KHR];
    for (index, dpb_slot) in reference_name_dpb_slot_indices.iter().copied().enumerate() {
        let Ok(dpb_slot) = u32::try_from(dpb_slot) else {
            continue;
        };
        if let Some(reference_slot_index) = unique_reference_slots
            .iter()
            .position(|slot| *slot == dpb_slot)
        {
            slots[index] = reference_slot_index as i32;
        }
    }
    slots
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_reference_info_from_active(
    reference: NativeVulkanAv1ActiveDpbReference,
    order_hint_offset_enabled: bool,
) -> vk::video::StdVideoDecodeAV1ReferenceInfo {
    vk::video::StdVideoDecodeAV1ReferenceInfo {
        flags: vk::video::StdVideoDecodeAV1ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(reference.disable_frame_end_update_cdf),
                native_vulkan_bool_u32(reference.segmentation_enabled),
                0,
            ),
        },
        frame_type: reference.frame_type,
        RefFrameSignBias: reference.ref_frame_sign_bias,
        OrderHint: reference.order_hint,
        SavedOrderHints: native_vulkan_av1_std_order_hints(
            reference.saved_order_hints,
            order_hint_offset_enabled,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_reference_info_from_decode_info(
    decode_info: &NativeVulkanAv1FirstFrameDecodeInfo,
    ref_frame_sign_bias: u8,
    saved_order_hints: [u8; 8],
    order_hint_offset_enabled: bool,
) -> vk::video::StdVideoDecodeAV1ReferenceInfo {
    vk::video::StdVideoDecodeAV1ReferenceInfo {
        flags: vk::video::StdVideoDecodeAV1ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeAV1ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(decode_info.header.disable_frame_end_update_cdf),
                native_vulkan_bool_u32(decode_info.header.segmentation.enabled),
                0,
            ),
        },
        frame_type: decode_info.header.frame_type,
        RefFrameSignBias: ref_frame_sign_bias,
        OrderHint: decode_info.header.order_hint.unwrap_or(0),
        SavedOrderHints: native_vulkan_av1_std_order_hints(
            saved_order_hints,
            order_hint_offset_enabled,
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) struct NativeVulkanAv1TemporalUnitExtract {
    payload: NativeVulkanEncodedAccessUnitPayload,
    pts_ns: Option<u64>,
    duration_ns: Option<u64>,
    pts_ms: Option<u64>,
    duration_ms: Option<u64>,
    stats: NativeVulkanAv1ObuStats,
}

#[cfg(feature = "native-vulkan-video")]
pub(super) type NativeVulkanH264StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanH264AccessUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
pub(super) type NativeVulkanH265StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanH265AccessUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
pub(super) type NativeVulkanAv1StreamingPacketQueue =
    NativeVulkanStreamingPacketQueue<NativeVulkanAv1TemporalUnitExtract>;

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanH264AccessUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::H264;

    pub(super) fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "H.264 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats = native_vulkan_h264_nal_stats(payload.bytes());
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanStreamingAccessUnit for NativeVulkanH265AccessUnitExtract {
    type ParameterSets = NativeVulkanH265ParameterSetSnapshot;
    type Snapshot = NativeVulkanH265AccessUnitSnapshot;

    const CODEC_LABEL: &'static str = "H.265";
    const PARAMETER_SETS_LABEL: &'static str = "VPS/SPS/PPS";

    pub(super) fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
        native_vulkan_parse_h265_parameter_sets(bytes)
    }

    pub(super) fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot {
        native_vulkan_h265_access_unit_snapshot(index, access_unit, parameter_sets)
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.payload.bytes()
    }

    pub(super) fn pts_ms(&self) -> Option<u64> {
        self.pts_ms
    }

    pub(super) fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    pub(super) fn has_parameter_sets(&self) -> bool {
        self.stats.parameter_sets_present()
    }

    pub(super) fn is_random_access(&self) -> bool {
        self.stats.idr_count > 0
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanH265AccessUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::H265;

    pub(super) fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "H.265 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats = native_vulkan_h265_nal_stats(payload.bytes());
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanStreamingAccessUnit for NativeVulkanAv1TemporalUnitExtract {
    type ParameterSets = NativeVulkanAv1SequenceHeaderSnapshot;
    type Snapshot = NativeVulkanAv1TemporalUnitSnapshot;

    const CODEC_LABEL: &'static str = "AV1";
    const PARAMETER_SETS_LABEL: &'static str = "sequence header";

    pub(super) fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
        native_vulkan_av1_obu_stats(bytes)?
            .sequence_header
            .ok_or_else(|| "AV1 temporal unit has no sequence header".to_owned())
    }

    pub(super) fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot {
        native_vulkan_av1_temporal_unit_snapshot(index, access_unit, Some(parameter_sets))
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.payload.bytes()
    }

    pub(super) fn pts_ms(&self) -> Option<u64> {
        self.pts_ms
    }

    pub(super) fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    pub(super) fn has_parameter_sets(&self) -> bool {
        self.stats.sequence_header_present()
    }

    pub(super) fn is_random_access(&self) -> bool {
        self.stats
            .first_frame_submit
            .as_ref()
            .is_some_and(|submit| {
                submit.frame_type == 0 && submit.show_frame && submit.vulkan_submit_candidate
            })
    }

    pub(super) fn is_random_access_with_parameter_sets(&self, parameter_sets: &Self::ParameterSets) -> bool {
        self.stats
            .first_frame_submit
            .clone()
            .or_else(|| {
                native_vulkan_av1_first_frame_submit_snapshot(
                    self.payload.bytes(),
                    &self.stats.obus,
                    parameter_sets,
                )
            })
            .is_some_and(|submit| {
                submit.frame_type == 0 && submit.show_frame && submit.vulkan_submit_candidate
            })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegStreamingAccessUnit for NativeVulkanAv1TemporalUnitExtract {
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec = NativeVulkanFfmpegCodec::Av1;
    const FFMPEG_PACKET_SPLITS_ACCESS_UNITS: bool = true;

    pub(super) fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError> {
        let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(payload);
        if payload.is_empty() {
            return Err(NativeVulkanError::Video(
                "AV1 FFmpeg packet is empty".to_owned(),
            ));
        }
        let stats =
            native_vulkan_av1_obu_stats(payload.bytes()).map_err(NativeVulkanError::Video)?;
        Ok(Self {
            payload,
            pts_ns: metadata.pts_ns,
            duration_ns: metadata.duration_ns,
            pts_ms: metadata.pts_ms,
            duration_ms: metadata.duration_ms,
            stats,
        })
    }

    pub(super) fn from_ffmpeg_packet_many(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Vec<Self>, NativeVulkanError> {
        let ranges = native_vulkan_av1_split_ffmpeg_packet_frame_ranges(payload.bytes())
            .map_err(NativeVulkanError::Video)?;
        payload
            .split_into_ranges(ranges, "AV1")?
            .into_iter()
            .map(|unit| {
                let payload = NativeVulkanEncodedAccessUnitPayload::from_ffmpeg_packet(unit);
                if payload.is_empty() {
                    return Err(NativeVulkanError::Video(
                        "AV1 FFmpeg packet frame unit is empty".to_owned(),
                    ));
                }
                let stats = native_vulkan_av1_obu_stats(payload.bytes())
                    .map_err(NativeVulkanError::Video)?;
                Ok(Self {
                    payload,
                    pts_ns: metadata.pts_ns,
                    duration_ns: metadata.duration_ns,
                    pts_ms: metadata.pts_ms,
                    duration_ms: metadata.duration_ms,
                    stats,
                })
            })
            .collect()
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_av1_temporal_unit_snapshot(
    index: u32,
    temporal_unit: &NativeVulkanAv1TemporalUnitExtract,
    active_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> NativeVulkanAv1TemporalUnitSnapshot {
    let first_frame_submit = temporal_unit.stats.first_frame_submit.clone().or_else(|| {
        let sequence_header = temporal_unit
            .stats
            .sequence_header
            .as_ref()
            .or(active_sequence_header)?;
        native_vulkan_av1_first_frame_submit_snapshot(
            temporal_unit.payload.bytes(),
            &temporal_unit.stats.obus,
            sequence_header,
        )
    });

    NativeVulkanAv1TemporalUnitSnapshot {
        index,
        bytes: temporal_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: temporal_unit.pts_ns,
        duration_ns: temporal_unit.duration_ns,
        pts_ms: temporal_unit.pts_ms,
        duration_ms: temporal_unit.duration_ms,
        obu_count: temporal_unit.stats.obu_count,
        sequence_header_count: temporal_unit.stats.sequence_header_count,
        temporal_delimiter_count: temporal_unit.stats.temporal_delimiter_count,
        frame_header_count: temporal_unit.stats.frame_header_count,
        tile_group_count: temporal_unit.stats.tile_group_count,
        frame_count: temporal_unit.stats.frame_count,
        decode_candidate: temporal_unit.stats.decode_candidate(),
        tile_payload_bytes: temporal_unit.stats.tile_payload_bytes,
        frame_payload_bytes: temporal_unit.stats.frame_payload_bytes,
        first_frame_header_obu_offset: temporal_unit.stats.first_frame_header_obu_offset,
        first_tile_group_obu_offset: temporal_unit.stats.first_tile_group_obu_offset,
        sequence_header_present: temporal_unit.stats.sequence_header_present(),
        sequence_header: temporal_unit.stats.sequence_header.clone(),
        first_frame_submit,
        obus: temporal_unit.stats.obus.clone(),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_h264_access_unit_snapshot(
    index: u32,
    access_unit: &NativeVulkanH264AccessUnitExtract,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> NativeVulkanH264AccessUnitSnapshot {
    let first_frame = native_vulkan_h264_picture_decode_info_from_stats(
        access_unit.payload.bytes(),
        &access_unit.stats,
        parameter_sets,
    );
    let (first_slice, first_slice_parse_error) = match first_frame {
        Ok(first_frame) => (
            Some(NativeVulkanH264AccessUnitSliceSnapshot {
                nal_type: first_frame.nal_type,
                nal_type_label: first_frame.nal_type_label,
                nal_ref_idc: first_frame.nal_ref_idc,
                first_mb_in_slice: first_frame.first_mb_in_slice,
                first_slice_segment_in_pic_flag: first_frame.first_slice_segment_in_pic_flag,
                slice_type: first_frame.slice_type,
                slice_type_normalized: first_frame.slice_type_normalized,
                pps_id: first_frame.pps_id,
                frame_num: first_frame.frame_num,
                idr_pic_id: first_frame.idr_pic_id,
                num_ref_idx_l0_active_minus1: first_frame.num_ref_idx_l0_active_minus1,
                num_ref_idx_l1_active_minus1: first_frame.num_ref_idx_l1_active_minus1,
                ref_pic_list_modification_l0: first_frame.ref_pic_list_modification_l0,
                ref_pic_list_modifications_l0: first_frame.ref_pic_list_modifications_l0,
                ref_pic_list_modification_l1: first_frame.ref_pic_list_modification_l1,
                ref_pic_list_modifications_l1: first_frame.ref_pic_list_modifications_l1,
                adaptive_ref_pic_marking_mode_flag: first_frame.adaptive_ref_pic_marking_mode_flag,
                memory_management_control_operations: first_frame
                    .memory_management_control_operations,
                field_pic_flag: first_frame.field_pic_flag,
                bottom_field_flag: first_frame.bottom_field_flag,
                is_reference: first_frame.is_reference,
                is_intra: first_frame.is_intra,
                is_p: first_frame.is_p,
                is_b: first_frame.is_b,
                long_term_reference_flag: first_frame.long_term_reference_flag,
                pic_order_cnt: first_frame.pic_order_cnt,
                slice_offsets: first_frame.slice_offsets,
                idr: first_frame.idr,
                irap: first_frame.irap,
            }),
            None,
        ),
        Err(err) => (None, Some(err)),
    };
    let idr_decode_ready = first_slice.as_ref().is_some_and(|slice| {
        slice.idr
            && slice.irap
            && slice.is_intra
            && !slice.field_pic_flag
            && !slice.slice_offsets.is_empty()
    });
    let decode_ready = first_slice.as_ref().is_some_and(|slice| {
        let active_l0_refs = slice
            .num_ref_idx_l0_active_minus1
            .map(|value| value.saturating_add(1))
            .unwrap_or(0);
        !slice.field_pic_flag
            && slice.is_reference
            && !slice.slice_offsets.is_empty()
            && !slice.is_b
            && !slice.long_term_reference_flag
            && native_vulkan_h264_ref_pic_list_modifications_supported(slice)
            && !slice.adaptive_ref_pic_marking_mode_flag
            && (slice.is_intra || (slice.is_p && active_l0_refs > 0))
    });

    NativeVulkanH264AccessUnitSnapshot {
        index,
        bytes: access_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: access_unit.pts_ns,
        duration_ns: access_unit.duration_ns,
        pts_ms: access_unit.pts_ms,
        duration_ms: access_unit.duration_ms,
        has_annex_b_start_codes: access_unit.stats.has_annex_b_start_codes,
        has_parameter_sets: access_unit.stats.parameter_sets_present(),
        h264_sps_count: access_unit.stats.sps_count,
        h264_pps_count: access_unit.stats.pps_count,
        h264_idr_count: access_unit.stats.idr_count,
        h264_slice_count: access_unit.stats.slice_count,
        first_slice,
        first_slice_parse_error,
        idr_decode_ready,
        decode_ready,
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_h265_access_unit_snapshot(
    index: u32,
    access_unit: &NativeVulkanH265AccessUnitExtract,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> NativeVulkanH265AccessUnitSnapshot {
    let first_slice_result = native_vulkan_h265_first_slice_probe_snapshot_from_stats(
        access_unit.payload.bytes(),
        &access_unit.stats,
        parameter_sets,
    );
    let (first_slice, first_slice_parse_error) = match first_slice_result {
        Ok(snapshot) => (Some(snapshot), None),
        Err(err) => (None, Some(err)),
    };
    NativeVulkanH265AccessUnitSnapshot {
        index,
        bytes: access_unit.stats.bytes,
        byte_hash: 0,
        pts_ns: access_unit.pts_ns,
        duration_ns: access_unit.duration_ns,
        pts_ms: access_unit.pts_ms,
        duration_ms: access_unit.duration_ms,
        has_annex_b_start_codes: access_unit.stats.has_annex_b_start_codes,
        has_parameter_sets: access_unit.stats.parameter_sets_present(),
        h265_vps_count: access_unit.stats.vps_count,
        h265_sps_count: access_unit.stats.sps_count,
        h265_pps_count: access_unit.stats.pps_count,
        h265_idr_count: access_unit.stats.idr_count,
        h265_slice_count: access_unit.stats.slice_count,
        first_slice,
        first_slice_parse_error,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_sps_short_term_ref_pic_sets_supported(
    ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> bool {
    ref_pic_sets.iter().all(|ref_pic_set| {
        ref_pic_set.num_negative_pics <= 16
            && ref_pic_set.num_positive_pics <= 16
            && ref_pic_set.use_delta_flags.len() <= 16
            && ref_pic_set.used_by_current_flags.len() <= 16
            && ref_pic_set
                .abs_delta_rps_minus1
                .is_none_or(|value| value <= u16::MAX as u32)
            && ref_pic_set
                .negative_delta_pocs
                .iter()
                .chain(ref_pic_set.positive_delta_pocs.iter())
                .all(|delta_poc| delta_poc.unsigned_abs() <= u16::MAX as u32)
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_sps_long_term_ref_pics_supported(
    ref_pics: &[NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> bool {
    ref_pics.len() <= 32
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_h264_sps_max_frame_num(sps: &NativeVulkanH264SpsSnapshot) -> u32 {
    1u32.checked_shl(sps.log2_max_frame_num_minus4.saturating_add(4))
        .unwrap_or(u32::MAX)
        .max(1)
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_h265_sps_max_pic_order_cnt_lsb(sps: &NativeVulkanH265SpsSnapshot) -> u32 {
    1u32.checked_shl(sps.log2_max_pic_order_cnt_lsb_minus4.saturating_add(4))
        .unwrap_or(u32::MAX)
        .max(1)
}
