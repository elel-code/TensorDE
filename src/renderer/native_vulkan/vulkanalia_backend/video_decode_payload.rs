use super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1ReadyPrefixFrameInput;
use super::video_decode_submit_h264::NativeVulkanVulkanaliaH264ReadyPrefixFrameInput;
use super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReadyPrefixFrameInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaH264FrameBitstream {
    pub(super) src_buffer_offset: u64,
    pub(super) src_buffer_range: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaH265FrameBitstream {
    pub(super) src_buffer_offset: u64,
    pub(super) src_buffer_range: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaAv1FrameBitstream {
    pub(super) src_buffer_offset: u64,
    pub(super) src_buffer_range: u64,
}

pub(super) fn native_vulkan_vulkanalia_h264_decode_payloads(
    frames: &mut [NativeVulkanVulkanaliaH264ReadyPrefixFrameInput],
    min_offset_alignment: u64,
    min_size_alignment: u64,
) -> Result<(Vec<u8>, Vec<NativeVulkanVulkanaliaH264FrameBitstream>), String> {
    if frames.is_empty() {
        return Err("Vulkanalia H.264 decode payload set cannot be empty".to_owned());
    }

    let mut bytes = Vec::with_capacity(native_vulkan_vulkanalia_decode_payload_capacity(
        frames
            .iter()
            .map(|frame| frame.access_unit_payload.len() as u64),
        min_offset_alignment,
        min_size_alignment,
    )?);
    let mut bitstreams = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.access_unit_payload.is_empty() {
            return Err(format!(
                "Vulkanalia H.264 AU {} decode payload cannot be empty",
                frame.entry.access_unit_index
            ));
        }
        let mut payload = std::mem::take(&mut frame.access_unit_payload);
        let src_buffer_offset =
            native_vulkan_vulkanalia_align_up(bytes.len() as u64, min_offset_alignment.max(1))?;
        bytes.resize(src_buffer_offset as usize, 0);
        let src_buffer_range =
            native_vulkan_vulkanalia_align_up(payload.len() as u64, min_size_alignment.max(1))?;
        bytes.append(&mut payload);
        bytes.resize((src_buffer_offset + src_buffer_range) as usize, 0);
        bitstreams.push(NativeVulkanVulkanaliaH264FrameBitstream {
            src_buffer_offset,
            src_buffer_range,
        });
    }
    Ok((bytes, bitstreams))
}

pub(super) fn native_vulkan_vulkanalia_h265_decode_payloads(
    frames: &mut [NativeVulkanVulkanaliaH265ReadyPrefixFrameInput],
    min_offset_alignment: u64,
    min_size_alignment: u64,
) -> Result<(Vec<u8>, Vec<NativeVulkanVulkanaliaH265FrameBitstream>), String> {
    if frames.is_empty() {
        return Err("Vulkanalia H.265 decode payload set cannot be empty".to_owned());
    }

    let mut bytes = Vec::with_capacity(native_vulkan_vulkanalia_decode_payload_capacity(
        frames
            .iter()
            .map(|frame| frame.access_unit_payload.len() as u64),
        min_offset_alignment,
        min_size_alignment,
    )?);
    let mut bitstreams = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.access_unit_payload.is_empty() {
            return Err(format!(
                "Vulkanalia H.265 AU {} decode payload cannot be empty",
                frame.entry.access_unit_index
            ));
        }
        let mut payload = std::mem::take(&mut frame.access_unit_payload);
        let src_buffer_offset =
            native_vulkan_vulkanalia_align_up(bytes.len() as u64, min_offset_alignment.max(1))?;
        bytes.resize(src_buffer_offset as usize, 0);
        let src_buffer_range =
            native_vulkan_vulkanalia_align_up(payload.len() as u64, min_size_alignment.max(1))?;
        bytes.append(&mut payload);
        bytes.resize((src_buffer_offset + src_buffer_range) as usize, 0);
        bitstreams.push(NativeVulkanVulkanaliaH265FrameBitstream {
            src_buffer_offset,
            src_buffer_range,
        });
    }
    Ok((bytes, bitstreams))
}

pub(super) fn native_vulkan_vulkanalia_av1_decode_payloads(
    frames: &mut [NativeVulkanVulkanaliaAv1ReadyPrefixFrameInput],
    min_offset_alignment: u64,
    min_size_alignment: u64,
) -> Result<(Vec<u8>, Vec<NativeVulkanVulkanaliaAv1FrameBitstream>), String> {
    if frames.is_empty() {
        return Err("Vulkanalia AV1 decode payload set cannot be empty".to_owned());
    }

    let mut bytes = Vec::with_capacity(native_vulkan_vulkanalia_decode_payload_capacity(
        frames
            .iter()
            .map(|frame| frame.access_unit_payload.len() as u64),
        min_offset_alignment,
        min_size_alignment,
    )?);
    let mut bitstreams = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.access_unit_payload.is_empty() {
            return Err(format!(
                "Vulkanalia AV1 TU {} decode payload cannot be empty",
                frame.entry.temporal_unit_index
            ));
        }
        let mut payload = std::mem::take(&mut frame.access_unit_payload);
        let src_buffer_offset =
            native_vulkan_vulkanalia_align_up(bytes.len() as u64, min_offset_alignment.max(1))?;
        bytes.resize(src_buffer_offset as usize, 0);
        let src_buffer_range =
            native_vulkan_vulkanalia_align_up(payload.len() as u64, min_size_alignment.max(1))?;
        bytes.append(&mut payload);
        bytes.resize((src_buffer_offset + src_buffer_range) as usize, 0);
        bitstreams.push(NativeVulkanVulkanaliaAv1FrameBitstream {
            src_buffer_offset,
            src_buffer_range,
        });
    }
    Ok((bytes, bitstreams))
}

fn native_vulkan_vulkanalia_decode_payload_capacity<I>(
    payload_lengths: I,
    min_offset_alignment: u64,
    min_size_alignment: u64,
) -> Result<usize, String>
where
    I: IntoIterator<Item = u64>,
{
    let mut total = 0u64;
    for payload_len in payload_lengths {
        let offset = native_vulkan_vulkanalia_align_up(total, min_offset_alignment.max(1))?;
        let range = native_vulkan_vulkanalia_align_up(payload_len, min_size_alignment.max(1))?;
        total = offset
            .checked_add(range)
            .ok_or_else(|| "Vulkanalia payload capacity overflow".to_owned())?;
    }
    usize::try_from(total).map_err(|_| "Vulkanalia payload capacity exceeds usize".to_owned())
}

fn native_vulkan_vulkanalia_align_up(value: u64, alignment: u64) -> Result<u64, String> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment.saturating_sub(1))
        .map(|aligned| aligned / alignment * alignment)
        .ok_or_else(|| "Vulkanalia alignment overflow".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::{
        NativeVulkanH265AccessUnitSliceSnapshot, NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    };

    #[test]
    fn h265_multi_frame_payloads_align_offsets_and_ranges() {
        let mut frames = vec![
            test_h265_ready_prefix_frame(0, vec![1, 2, 3]),
            test_h265_ready_prefix_frame(1, vec![4; 260]),
        ];

        let (bytes, bitstreams) =
            native_vulkan_vulkanalia_h265_decode_payloads(&mut frames, 128, 256).unwrap();

        assert_eq!(bitstreams.len(), 2);
        assert_eq!(bitstreams[0].src_buffer_offset, 0);
        assert_eq!(bitstreams[0].src_buffer_range, 256);
        assert_eq!(bitstreams[1].src_buffer_offset, 256);
        assert_eq!(bitstreams[1].src_buffer_range, 512);
        assert_eq!(bytes.len(), 768);
        assert_eq!(&bytes[..3], &[1, 2, 3]);
        assert_eq!(&bytes[256..260], &[4, 4, 4, 4]);
        assert!(
            frames
                .iter()
                .all(|frame| frame.access_unit_payload.is_empty())
        );
    }

    fn test_h265_ready_prefix_frame(
        access_unit_index: u32,
        access_unit_payload: Vec<u8>,
    ) -> NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
        NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
            entry: NativeVulkanH265DecodeReferencePlanEntrySnapshot {
                access_unit_index,
                pts_ms: None,
                nal_type_label: None,
                current_poc: Some(access_unit_index as i32),
                planned_output_slot: access_unit_index,
                setup_slot_index: None,
                evicted_poc: None,
                references: Vec::new(),
                available_reference_count: 0,
                missing_reference_count: 0,
                missing_reference_pocs: Vec::new(),
                ready_for_decode_submit: true,
            },
            first_slice: NativeVulkanH265AccessUnitSliceSnapshot {
                nal_type: 1,
                nal_type_label: "TRAIL_R",
                slice_segment_offset: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: 1,
                pps_id: 0,
                pic_order_cnt_lsb: Some(0),
                short_term_ref_pic_set_sps_flag: false,
                short_term_ref_pic_set_idx: None,
                num_delta_pocs_of_ref_rps_idx: 0,
                num_bits_for_st_ref_pic_set_in_slice: 0,
                short_term_ref_pic_set: None,
                long_term_references: Vec::new(),
                idr: false,
                irap: false,
            },
            duration_ms: None,
            access_unit_payload,
            slice_segment_offset: 0,
        }
    }
}
