//! WE `0x14020ae00` payload contract behind `[layer+0x490]` RT method [8].
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`

use serde::Serialize;

use crate::engine::scene_engine::SceneLayerAlphaMaskRtMethod8MdlvGeometry;

pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_PAYLOAD_REBUILD_VMA: &str =
    "0x14020ae00";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_COPY_SCALE_REGION: &str =
    "0x14020af31..0x14020b102";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_AUX_PAYLOAD_REGION: &str =
    "0x14020b214..0x14020b66f";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_SLICE_HELPER_NO_TOKEN_VMA: &str =
    "0x14020c710";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_SLICE_HELPER_APPEND_TOKEN0_VMA:
    &str = "0x14020c850";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST: u32 = 0x1;
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST_MODIFIER:
    u32 = 0x4;
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_AUX_FLAG_SECOND_LIST: u32 = 0x8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8PayloadPlan {
    pub rebuild_function_vma: &'static str,
    pub entry_source: &'static str,
    pub entry_field_count: usize,
    pub entry_fields: [NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField; 9],
    pub copy_scale_region: &'static str,
    pub copy_scale_gate_count: usize,
    pub copy_scale_gates: [NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate; 4],
    pub copy_scale_operation: &'static str,
    pub local_reference: NativeVulkanSceneLayerAlphaMaskRtMethod8LocalReference,
    pub aux_payload_region: &'static str,
    pub aux_vector_target: &'static str,
    pub aux_record_size_bytes: u32,
    pub aux_record_count_source: &'static str,
    pub aux_record_fields: [NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField; 7],
    pub aux_flag_source_count: usize,
    pub aux_flag_sources: [NativeVulkanSceneLayerAlphaMaskRtMethod8AuxFlagSource; 3],
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
    pub field: &'static str,
    pub semantic: &'static str,
    pub consumer: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate
{
    pub branch_site: &'static str,
    pub condition: &'static str,
    pub outcome_when_true: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8LocalReference
{
    pub object: &'static str,
    pub skip_copy_reason: &'static str,
    pub file_layout_mask: &'static str,
    pub entry_layout_key: &'static str,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub entry_flags: &'static str,
    pub wrapper_usage_arg9: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField
{
    pub record_offset: &'static str,
    pub value: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8AuxFlagSource
{
    pub range: &'static str,
    pub source: &'static str,
    pub flag: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadLoweringPlan
{
    pub source_record_count: usize,
    pub subdraw_count: usize,
    pub aux_record_count: usize,
    pub records: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadRecord>,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadRecord
{
    pub payload_index: usize,
    pub source_index: u32,
    pub local_offset: u32,
    pub index_span_offset: u32,
    pub index_span_count: u32,
    pub ordinal: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSlicePlan
{
    pub helper_vma: &'static str,
    pub draw_index: u32,
    pub appends_token_zero: bool,
    pub payload_indices: Vec<usize>,
    pub index_count: u32,
    pub index_payload: Vec<u8>,
    pub copied_spans: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSliceSpan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSliceSpan
{
    pub payload_index: usize,
    pub source_index_offset: u32,
    pub index_count: u32,
    pub byte_offset: u32,
    pub byte_count: u32,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan()
-> NativeVulkanSceneLayerAlphaMaskRtMethod8PayloadPlan {
    NativeVulkanSceneLayerAlphaMaskRtMethod8PayloadPlan {
        rebuild_function_vma: LAYER_490_RT_METHOD8_PAYLOAD_REBUILD_VMA,
        entry_source: "entry = [[layer+0x4b8]+0x18], the first/current 0xc8 MDLV entry-owner",
        entry_field_count: 9,
        entry_fields: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x18",
                semantic: "entry flags",
                consumer: "wrapper stack arg 9 uses ((flags >> 3) & 1) * 2",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x38",
                semantic: "vertex layout key",
                consumer: "wrapper [8] edx",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x3c",
                semantic: "vertex stride bytes",
                consumer: "vertex count divides +0x40 by this stride",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x40",
                semantic: "vertex byte count",
                consumer: "wrapper [8] r9d = +0x40 / +0x3c",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x48",
                semantic: "MDLV vertex payload",
                consumer: "wrapper [8] r8 unless the copy/scale branch replaces it",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x50",
                semantic: "index byte count",
                consumer: "wrapper [8] stack arg 6 = +0x50 / 2",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0x58",
                semantic: "MDLV u16 index payload",
                consumer: "wrapper [8] stack arg 5",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0xa0",
                semantic: "source/subdraw record count",
                consumer: "aux+0x298 reserve count",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8EntryField {
                field: "+0xa8",
                semantic: "16-byte source records",
                consumer: "aux+0x298 0x1c record rebuild",
            },
        ],
        copy_scale_region: LAYER_490_RT_METHOD8_COPY_SCALE_REGION,
        copy_scale_gate_count: 4,
        copy_scale_gates: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate {
                branch_site: "0x14020af31",
                condition: "[layer+0x320] > 0",
                outcome_when_true: "skip temporary copy/scale and upload original MDLV vertex payload",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate {
                branch_site: "0x14020af3e",
                condition: "[layer+0x304] bit 0x10 is set",
                outcome_when_true: "skip temporary copy/scale and upload original MDLV vertex payload",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate {
                branch_site: "0x14020af50",
                condition: "first_pass_entry+0x1c bit 0x4 is set",
                outcome_when_true: "skip temporary copy/scale and upload original MDLV vertex payload",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CopyScaleGate {
                branch_site: "0x14020af71",
                condition: "active_entry_bool is true",
                outcome_when_true: "skip temporary copy/scale and upload original MDLV vertex payload",
            },
        ],
        copy_scale_operation: "when all gates are false, copy entry+0x48 vertex bytes, find position component offsets from the WE layout tables, multiply x by xmm7 and multiply y by xmm6, then pass the temporary copy as wrapper r8",
        local_reference: NativeVulkanSceneLayerAlphaMaskRtMethod8LocalReference {
            object: "3742497499 object 1530 eye puppet",
            skip_copy_reason: "[object+0x320] > 0 takes 0x14020af31 -> 0x14020b107",
            file_layout_mask: "0x1800009",
            entry_layout_key: "0x180000f",
            vertex_stride_bytes: 80,
            vertex_count: 4106,
            index_count: 23988,
            entry_flags: "0x4",
            wrapper_usage_arg9: "0",
        },
        aux_payload_region: LAYER_490_RT_METHOD8_AUX_PAYLOAD_REGION,
        aux_vector_target: "aux+0x298",
        aux_record_size_bytes: 0x1c,
        aux_record_count_source: "[entry+0xa0] records from entry+0xa8",
        aux_record_fields: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x00",
                value: "0",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x04",
                value: "aux+0x1f0[record.source_index] + record.local_offset",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x08",
                value: "record.source_index from entry+0xa8 + i*0x10 + 0",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x0c",
                value: "entry+0xa8 + i*0x10 + 0x08",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x10",
                value: "entry+0xa8 + i*0x10 + 0x0c, default initialized to 1",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x14",
                value: "source-record ordinal i",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxRecordField {
                record_offset: "+0x18",
                value: "flags built from subdraw index lists",
            },
        ],
        aux_flag_source_count: 3,
        aux_flag_sources: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxFlagSource {
                range: "0x14020b5c0..0x14020b5f2",
                source: "subdraw +0x08..+0x10 index list",
                flag: "OR +0x18 with 0x1",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxFlagSource {
                range: "0x14020b5f2..0x14020b622",
                source: "subdraw +0x20..+0x28 index list",
                flag: "OR +0x18 with 0x8",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8AuxFlagSource {
                range: "0x14020b622..0x14020b662",
                source: "subdraw +0x44 bit 0x8 plus the +0x08 index list",
                flag: "OR +0x18 with 0x4",
            },
        ],
        command_order: [
            "release_previous_layer_0x490_and_0x3f8_targets",
            "read_first_current_mdlv_entry_owner_from_layer_0x4b8_plus_0x18",
            "derive_wrapper_8_geometry_arguments_from_entry_fields",
            "optionally_copy_and_scale_vertex_xy_before_upload",
            "create_layer_0x490_indexed_rt_target",
            "rebuild_aux_0x298_per_source_records_from_entry_0xa8",
            "apply_subdraw_index_list_flags_0x1_0x8_0x4",
            "feed_token_52_53_0x14020cff0_and_0x14020d6a0_consumers",
        ],
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload(
    geometry: &SceneLayerAlphaMaskRtMethod8MdlvGeometry,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadLoweringPlan, String> {
    let mut records = geometry
        .source_records
        .iter()
        .enumerate()
        .map(
            |(payload_index, record)| NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadRecord {
                payload_index,
                source_index: record.source_index,
                local_offset: record.local_offset,
                index_span_offset: record.index_span_offset,
                index_span_count: record.index_span_count,
                ordinal: payload_index.min(u32::MAX as usize) as u32,
                flags: 0,
            },
        )
        .collect::<Vec<_>>();

    for (subdraw_index, subdraw) in geometry.subdraws.iter().enumerate() {
        for payload_index in &subdraw.first_indices {
            let record = aux_record_mut(&mut records, *payload_index, subdraw_index)?;
            record.flags |= LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST;
            if subdraw.raw_flags & 0x8 != 0 {
                record.flags |= LAYER_490_RT_METHOD8_AUX_FLAG_FIRST_LIST_MODIFIER;
            }
        }
        for payload_index in &subdraw.second_indices {
            aux_record_mut(&mut records, *payload_index, subdraw_index)?.flags |=
                LAYER_490_RT_METHOD8_AUX_FLAG_SECOND_LIST;
        }
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadLoweringPlan {
            source_record_count: geometry.source_records.len(),
            subdraw_count: geometry.subdraws.len(),
            aux_record_count: records.len(),
            records,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x14020b3e0..0x14020b446 allocates 0x1c aux+0x298 records from entry+0xa8",
                "reverse-engineered/docs/exe/clipping-pipeline.md: payload +0x0c/+0x10 are u16 index span offset/count",
                "reverse-engineered/docs/exe/clipping-pipeline.md: subdraw +0x08/+0x20/+0x44 set aux flags 0x1/0x8/0x4",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: 0x14020c710/0x14020c850 materialize R16 indexed slices, not SRVs",
            ],
            command_order: [
                "read_mdlv_optional_b_16_byte_source_records",
                "initialize_aux_0x298_records_with_source_index_local_offset_and_index_span",
                "apply_subdraw_first_list_flag_0x1",
                "apply_subdraw_second_list_flag_0x8",
                "apply_subdraw_raw_flag_0x8_modifier_0x4_to_first_list",
            ],
        },
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice(
    geometry: &SceneLayerAlphaMaskRtMethod8MdlvGeometry,
    aux_payload: &NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadLoweringPlan,
    draw_index: u32,
    payload_indices: &[usize],
    appends_token_zero: bool,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSlicePlan, String> {
    let mut index_payload = Vec::new();
    let mut copied_spans = Vec::with_capacity(payload_indices.len());
    let mut index_count = 0u32;
    for payload_index in payload_indices {
        let record = aux_payload.records.get(*payload_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] slice draw {draw_index} references missing aux+0x298 payload index {payload_index}"
            )
        })?;
        let byte_offset = record.index_span_offset.checked_mul(2).ok_or_else(|| {
            "scene layer alpha-mask RT method [8] slice byte offset overflowed".to_owned()
        })?;
        let byte_count = record.index_span_count.checked_mul(2).ok_or_else(|| {
            "scene layer alpha-mask RT method [8] slice byte count overflowed".to_owned()
        })?;
        let byte_end = byte_offset.checked_add(byte_count).ok_or_else(|| {
            "scene layer alpha-mask RT method [8] slice byte range overflowed".to_owned()
        })?;
        let source = geometry
            .index_payload
            .get(byte_offset as usize..byte_end as usize)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask RT method [8] slice draw {draw_index} payload index {payload_index} references R16 span outside MDLV index payload"
                )
            })?;
        index_payload.extend_from_slice(source);
        index_count = index_count
            .checked_add(record.index_span_count)
            .ok_or_else(|| {
                "scene layer alpha-mask RT method [8] slice index count overflowed".to_owned()
            })?;
        copied_spans.push(NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSliceSpan {
            payload_index: *payload_index,
            source_index_offset: record.index_span_offset,
            index_count: record.index_span_count,
            byte_offset,
            byte_count,
        });
    }

    Ok(NativeVulkanSceneLayerAlphaMaskRtMethod8IndexSlicePlan {
        helper_vma: if appends_token_zero {
            LAYER_490_RT_METHOD8_SLICE_HELPER_APPEND_TOKEN0_VMA
        } else {
            LAYER_490_RT_METHOD8_SLICE_HELPER_NO_TOKEN_VMA
        },
        draw_index,
        appends_token_zero,
        payload_indices: payload_indices.to_vec(),
        index_count,
        index_payload,
        copied_spans,
        command_order: [
            "clear_layer_0x490_slice_slot_with_vtable_plus_0x38",
            "configure_r16_slice_buffer_with_vtable_plus_0x30",
            "map_slice_buffer_with_vtable_plus_0x50",
            "copy_mdlv_index_payload_spans_from_entry_plus_0x58",
            "unmap_slice_buffer_with_vtable_plus_0x58",
            "append_token_0_only_for_0x14020c850",
        ],
    })
}

fn aux_record_mut(
    records: &mut [NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadRecord],
    payload_index: u32,
    subdraw_index: usize,
) -> Result<&mut NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadRecord, String> {
    let Some(index) = usize::try_from(payload_index).ok() else {
        return Err(format!(
            "scene layer alpha-mask subdraw {subdraw_index} aux payload index overflowed"
        ));
    };
    records.get_mut(index).ok_or_else(|| {
        format!(
            "scene layer alpha-mask subdraw {subdraw_index} references missing aux+0x298 payload index {payload_index}"
        )
    })
}

#[cfg(test)]
#[path = "rt_method8_payload_tests.rs"]
mod tests;
