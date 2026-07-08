//! WE auxiliary material target draw receiver contracts.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/tools/audit_opacity_final_alpha_path.py`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency, SceneObjectId, SceneResidentResource,
    SceneResourceResidencyPlan, WE_AUX_CLEAR_SOURCE_DIMENSION_REGION,
    WE_AUX_CLEAR_UV_FLIP_FLAG_SOURCE, WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET,
    WE_LAYER_AUX_EFFECT_TARGET_OFFSET, WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
    WE_LAYER_AUX_MATERIAL_TARGET_OFFSET,
};

use super::layer_aux_clear_prep::{
    NativeVulkanSceneLayerAuxClearPrepCommandPlan, NativeVulkanSceneLayerAuxClearPrepFramePlan,
};

pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_TARGET_CLEAR_CREATE_VMA: u64 =
    0x14020a3ea;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_TARGET_CLEAR_STORE_VMA: u64 =
    0x14020a3f0;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_TARGET_ACTIVE_CREATE_VMA: u64 =
    0x14020b1e8;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_TARGET_ACTIVE_STORE_VMA: u64 =
    0x14020b1f3;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_WRAPPER_CREATE_NON_INDEXED_VMA: u64 =
    0x14009a780;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_WRAPPER_CREATE_INDEXED_VMA: u64 =
    0x14009a880;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_VPTR: u64 = 0x140486f38;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_NORMAL_DRAW_VMA: u64 = 0x1400ea780;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_INDEXED_DRAW_VMA: u64 = 0x1400eacd0;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_LAYOUT_KEY_HELPER_VMA: u64 = 0x140098c30;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_POSITION_UV_ATTR_IDS: [u32; 2] = [0, 7];
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK: u32 = 0x9;
pub(in crate::renderer::native_vulkan) const WE_RT_TARGET_POSITION_UV_STRIDE_BYTES: u32 = 20;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT: u32 = 3;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES: u64 =
    WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT as u64 * WE_RT_TARGET_POSITION_UV_STRIDE_BYTES as u64;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_CLEAR_TRIANGLE_PAYLOAD_BYTES: usize =
    WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES as usize;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_CLEAR_TOPOLOGY_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_ACTIVE_INDEX_WIDTH_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_ACTIVE_TOPOLOGY_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_ACTIVE_STACK_USAGE_BYTE: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialDrawFramePlan {
    pub active_block_count: usize,
    pub command_count: usize,
    pub draw_receiver_count: usize,
    pub non_indexed_draw_receiver_count: usize,
    pub indexed_draw_receiver_count: usize,
    pub retained_active_geometry_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxMaterialDrawCommandPlan>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub clear_material: NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan,
    pub generated_material: NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan,
    pub reference_points: [&'static str; 6],
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
    pub receiver_kind: NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
    pub material_offset: u32,
    pub target_offset: u32,
    pub create_call_vma: u64,
    pub store_vma: u64,
    pub wrapper_create_vma: u64,
    pub target_vptr: u64,
    pub draw_method_vma: u64,
    pub layout_key_source: &'static str,
    pub vertex_payload_source: &'static str,
    pub index_payload_source: Option<&'static str>,
    pub layout_key_helper_vma: u64,
    pub attribute_ids: [u32; 2],
    pub layout_bitmask: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub vertex_bytes: u64,
    pub index_count: u32,
    pub index_width_selector: Option<u32>,
    pub topology_selector: u32,
    pub stack_usage_byte: Option<u32>,
    pub active_entry_owner_index: Option<u32>,
    pub retained_vertex_bytes: Option<u64>,
    pub retained_index_bytes: Option<u64>,
    pub clear_triangle_payload: Option<NativeVulkanSceneLayerAuxClearTrianglePayloadPlan>,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAuxMaterialDrawReceiverKind {
    Aux3f0ClearMaterialNonIndexed,
    Aux3f8GeneratedMaterialIndexed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearTrianglePayloadPlan {
    pub create_region: &'static str,
    pub position_constants_vma: [u64; 2],
    pub uv_formula_region: &'static str,
    pub flip_flag_source: &'static str,
    pub source_width: u32,
    pub source_height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub uv_y_flipped: bool,
    pub uv_x_scale_bits: u32,
    pub uv_y_scale_bits: u32,
    pub clip_positions_bits: [[u32; 3]; 3],
    pub uv_x_formula: [&'static str; 3],
    pub uv_y_normal_formula: [&'static str; 3],
    pub uv_y_flipped_formula: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearTrianglePayload {
    pub bytes: Vec<u8>,
    pub uv_x_scale_bits: u32,
    pub uv_y_scale_bits: u32,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_aux_clear_triangle_payload(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    uv_y_flipped: bool,
) -> Result<NativeVulkanSceneLayerAuxClearTrianglePayload, String> {
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return Err(format!(
            "scene layer aux clear triangle needs non-zero source/target dimensions, got source {}x{} target {}x{}",
            source_width, source_height, target_width, target_height
        ));
    }

    let uv_x_scale = 2.0f32 * (target_width as f32) / (source_width as f32);
    let uv_y_scale = (target_height as f32) / (source_height as f32);
    if !uv_x_scale.is_finite() || !uv_y_scale.is_finite() {
        return Err(format!(
            "scene layer aux clear triangle produced non-finite UV scale from source {}x{} target {}x{}",
            source_width, source_height, target_width, target_height
        ));
    }

    let uv_y_0 = if uv_y_flipped { 0.0 } else { uv_y_scale };
    let uv_y_1 = if uv_y_flipped {
        uv_x_scale
    } else {
        -uv_y_scale
    };
    let uv_y_2 = if uv_y_flipped { 0.0 } else { uv_y_scale };
    let mut bytes = Vec::with_capacity(WE_AUX_MATERIAL_CLEAR_TRIANGLE_PAYLOAD_BYTES);
    push_aux_clear_vertex(&mut bytes, -1.0, 1.0, 0.0, 0.0, uv_y_0);
    push_aux_clear_vertex(&mut bytes, -1.0, -3.0, 0.0, 0.0, uv_y_1);
    push_aux_clear_vertex(&mut bytes, 3.0, 1.0, 0.0, uv_x_scale, uv_y_2);
    Ok(NativeVulkanSceneLayerAuxClearTrianglePayload {
        bytes,
        uv_x_scale_bits: uv_x_scale.to_bits(),
        uv_y_scale_bits: uv_y_scale.to_bits(),
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_material_draws(
    clear_prep: &NativeVulkanSceneLayerAuxClearPrepFramePlan,
    residency: &SceneResourceResidencyPlan,
) -> Result<NativeVulkanSceneLayerAuxMaterialDrawFramePlan, String> {
    if clear_prep.command_count == 0 {
        return Ok(NativeVulkanSceneLayerAuxMaterialDrawFramePlan::empty());
    }

    let mut commands = Vec::with_capacity(clear_prep.command_count);
    for clear_command in &clear_prep.commands {
        let active_geometry = active_geometry_for_object(residency, clear_command.object)
            .ok_or_else(|| {
                format!(
                    "scene layer aux material draws object {:?} has no retained active-entry MDLV geometry for [aux+0x3f8]",
                    clear_command.object
                )
            })?;
        commands.push(
            NativeVulkanSceneLayerAuxMaterialDrawCommandPlan::from_clear_prep_command(
                clear_command,
                active_geometry,
            )?,
        );
    }

    Ok(NativeVulkanSceneLayerAuxMaterialDrawFramePlan::from_commands(commands))
}

impl NativeVulkanSceneLayerAuxMaterialDrawFramePlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            active_block_count: 0,
            command_count: 0,
            draw_receiver_count: 0,
            non_indexed_draw_receiver_count: 0,
            indexed_draw_receiver_count: 0,
            retained_active_geometry_count: 0,
            commands: Vec::new(),
            command_order: aux_material_draw_frame_order(),
        }
    }

    fn from_commands(commands: Vec<NativeVulkanSceneLayerAuxMaterialDrawCommandPlan>) -> Self {
        let non_indexed_draw_receiver_count = commands
            .iter()
            .filter(|command| {
                command.clear_material.receiver_kind
                    == NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
            })
            .count();
        let indexed_draw_receiver_count = commands
            .iter()
            .filter(|command| {
                command.generated_material.receiver_kind
                    == NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
            })
            .count();
        Self {
            active_block_count: commands.len(),
            command_count: commands.len(),
            draw_receiver_count: non_indexed_draw_receiver_count
                .saturating_add(indexed_draw_receiver_count),
            non_indexed_draw_receiver_count,
            indexed_draw_receiver_count,
            retained_active_geometry_count: indexed_draw_receiver_count,
            commands,
            command_order: aux_material_draw_frame_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn covers_clear_prep(
        &self,
        clear_prep: &NativeVulkanSceneLayerAuxClearPrepFramePlan,
    ) -> bool {
        self.active_block_count == clear_prep.active_block_count
            && self.command_count == clear_prep.command_count
            && clear_prep.commands.iter().all(|clear_command| {
                self.commands.iter().any(|draw_command| {
                    draw_command.block_index == clear_command.block_index
                        && draw_command.object == clear_command.object
                })
            })
    }
}

impl NativeVulkanSceneLayerAuxMaterialDrawCommandPlan {
    fn from_clear_prep_command(
        clear_prep: &NativeVulkanSceneLayerAuxClearPrepCommandPlan,
        active_geometry: SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency,
    ) -> Result<Self, String> {
        let clear_material =
            NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan::clear_material(clear_prep)?;
        let generated_material =
            NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan::generated_material(active_geometry)?;
        Ok(Self {
            command_index: clear_prep.command_index,
            block_index: clear_prep.block_index,
            object: clear_prep.object,
            clear_material,
            generated_material,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207740 draws [aux+0x410]->[aux+0x3f0] and [aux+0x408]->[aux+0x3f8]",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper [9]/+0x48 and wrapper [8]/+0x40 create target-like draw receivers",
                "reverse-engineered/tools/audit_opacity_final_alpha_path.py: 0x14020a3ea stores aux+0x3f0",
                "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020b1e8 stores aux+0x3f8 from active material entry",
                "0x14020a379..0x14020a390 releases the previous aux+0x3f0 before replacement",
                "references/godot/servers/rendering/rendering_device_graph.cpp: draw resources are explicit graph inputs before recording",
            ],
            command_order: [
                "load_aux_clear_prep_command",
                "materialize_aux_0x3f0_non_indexed_receiver_contract",
                "materialize_aux_0x3f8_indexed_receiver_contract",
                "require_retained_active_mdlv_geometry_for_aux_0x3f8",
                "preserve_wrapper_create_arguments",
                "feed_aux_clear_prep_recorder_without_mesh_owner",
                "keep_resource_heap_binding_model",
            ],
        })
    }
}

impl NativeVulkanSceneLayerAuxMaterialDrawReceiverPlan {
    fn clear_material(
        clear_prep: &NativeVulkanSceneLayerAuxClearPrepCommandPlan,
    ) -> Result<Self, String> {
        if clear_prep.clear_source_width == 0
            || clear_prep.clear_source_height == 0
            || clear_prep.clear_target_width == 0
            || clear_prep.clear_target_height == 0
        {
            return Err(format!(
                "scene layer aux material draw object {:?} has zero aux source/target extent",
                clear_prep.object
            ));
        }
        let clear_triangle_payload = native_vulkan_scene_layer_aux_clear_triangle_payload(
            clear_prep.clear_source_width,
            clear_prep.clear_source_height,
            clear_prep.clear_target_width,
            clear_prep.clear_target_height,
            clear_prep.clear_uv_y_flipped,
        )?;
        Ok(Self {
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed,
            material_offset: WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET,
            target_offset: WE_LAYER_AUX_MATERIAL_TARGET_OFFSET,
            create_call_vma: WE_AUX_MATERIAL_TARGET_CLEAR_CREATE_VMA,
            store_vma: WE_AUX_MATERIAL_TARGET_CLEAR_STORE_VMA,
            wrapper_create_vma: WE_RT_TARGET_WRAPPER_CREATE_NON_INDEXED_VMA,
            target_vptr: WE_RT_TARGET_VPTR,
            draw_method_vma: WE_RT_TARGET_NORMAL_DRAW_VMA,
            layout_key_source: "0x140098c30([0,7]) -> 0x9",
            vertex_payload_source: "stack triangle at 0x14020a2d2..0x14020a379",
            index_payload_source: None,
            layout_key_helper_vma: WE_RT_TARGET_LAYOUT_KEY_HELPER_VMA,
            attribute_ids: WE_RT_TARGET_POSITION_UV_ATTR_IDS,
            layout_bitmask: WE_RT_TARGET_POSITION_UV_LAYOUT_BITMASK,
            vertex_stride_bytes: WE_RT_TARGET_POSITION_UV_STRIDE_BYTES,
            vertex_count: WE_AUX_MATERIAL_CLEAR_VERTEX_COUNT,
            vertex_bytes: WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES,
            index_count: 0,
            index_width_selector: None,
            topology_selector: WE_AUX_MATERIAL_CLEAR_TOPOLOGY_SELECTOR,
            stack_usage_byte: Some(0),
            active_entry_owner_index: None,
            retained_vertex_bytes: Some(WE_AUX_MATERIAL_CLEAR_VERTEX_BYTES),
            retained_index_bytes: None,
            clear_triangle_payload: Some(aux_clear_triangle_payload_plan(
                clear_prep.clear_source_width,
                clear_prep.clear_source_height,
                clear_prep.clear_target_width,
                clear_prep.clear_target_height,
                clear_prep.clear_uv_y_flipped,
                clear_triangle_payload.uv_x_scale_bits,
                clear_triangle_payload.uv_y_scale_bits,
            )),
            reference_points: [
                "0x14020a2d2..0x14020a379 fills the 3*20-byte stack vertex payload",
                "0x14020a3ba..0x14020a3d2 computes layout key from attr ids [0,7]",
                "0x14020a3d4..0x14020a3ea calls wrapper +0x48 with r9d=3 and topology selector 0",
                "0x14020a3f0 stores the created receiver at [aux+0x3f0]",
            ],
            command_order: [
                "release_previous_aux_0x3f0_receiver",
                "build_position_uv_layout_key",
                "emit_three_vertex_position_uv_triangle",
                "create_non_indexed_target_like_receiver",
                "store_aux_0x3f0_receiver",
                "draw_under_aux_0x410_material_scope",
            ],
        })
    }

    fn generated_material(
        active_geometry: SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency,
    ) -> Result<Self, String> {
        if active_geometry.vertex_count == 0 || active_geometry.index_count == 0 {
            return Err(format!(
                "scene layer aux material draw object {:?} has empty active-entry geometry for [aux+0x3f8]",
                active_geometry.object
            ));
        }
        if active_geometry.vertex_stride_bytes == 0 {
            return Err(format!(
                "scene layer aux material draw object {:?} has zero active-entry vertex stride",
                active_geometry.object
            ));
        }
        Ok(Self {
            receiver_kind:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed,
            material_offset: WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
            target_offset: WE_LAYER_AUX_EFFECT_TARGET_OFFSET,
            create_call_vma: WE_AUX_MATERIAL_TARGET_ACTIVE_CREATE_VMA,
            store_vma: WE_AUX_MATERIAL_TARGET_ACTIVE_STORE_VMA,
            wrapper_create_vma: WE_RT_TARGET_WRAPPER_CREATE_INDEXED_VMA,
            target_vptr: WE_RT_TARGET_VPTR,
            draw_method_vma: WE_RT_TARGET_INDEXED_DRAW_VMA,
            layout_key_source: "[aux+0x18] + [aux+0x390] * 0xc8 + 0x38",
            vertex_payload_source: "active material entry +0x48, count +0x40/+0x3c",
            index_payload_source: Some("active material entry +0x58, count +0x50/2"),
            layout_key_helper_vma: 0,
            attribute_ids: [0, 0],
            layout_bitmask: active_geometry.layout_key,
            vertex_stride_bytes: active_geometry.vertex_stride_bytes,
            vertex_count: active_geometry.vertex_count,
            vertex_bytes: active_geometry.vertex_bytes,
            index_count: active_geometry.index_count,
            index_width_selector: Some(WE_AUX_MATERIAL_ACTIVE_INDEX_WIDTH_SELECTOR),
            topology_selector: WE_AUX_MATERIAL_ACTIVE_TOPOLOGY_SELECTOR,
            stack_usage_byte: Some(WE_AUX_MATERIAL_ACTIVE_STACK_USAGE_BYTE),
            active_entry_owner_index: Some(active_geometry.entry_owner_index),
            retained_vertex_bytes: Some(active_geometry.vertex_bytes),
            retained_index_bytes: Some(active_geometry.index_bytes),
            clear_triangle_payload: None,
            reference_points: [
                "0x14020b171 gates the active material upload from [aux+0x390]",
                "0x14020b17b..0x14020b1e3 passes active entry layout/vertex/index payload through wrapper [8]",
                "0x14020b182 forces stack usage byte 0; index/topology selectors are 0",
                "0x14020b1f3 stores the created receiver at [aux+0x3f8]",
            ],
            command_order: [
                "resolve_active_material_entry",
                "validate_retained_mdlv_vertex_index_payload",
                "create_indexed_target_like_receiver",
                "store_aux_0x3f8_receiver",
                "draw_under_aux_0x408_material_scope",
                "preserve_static_r16_triangle_list",
            ],
        })
    }
}

fn active_geometry_for_object(
    residency: &SceneResourceResidencyPlan,
    object: SceneObjectId,
) -> Option<SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency> {
    residency
        .resources
        .iter()
        .find_map(|resource| match resource {
            SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(geometry)
                if geometry.object == object =>
            {
                Some(*geometry)
            }
            _ => None,
        })
}

fn aux_clear_triangle_payload_plan(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
    uv_y_flipped: bool,
    uv_x_scale_bits: u32,
    uv_y_scale_bits: u32,
) -> NativeVulkanSceneLayerAuxClearTrianglePayloadPlan {
    NativeVulkanSceneLayerAuxClearTrianglePayloadPlan {
        create_region: "0x14020a2d2..0x14020a379",
        position_constants_vma: [0x140492aa0, 0x140492af0],
        uv_formula_region: WE_AUX_CLEAR_SOURCE_DIMENSION_REGION,
        flip_flag_source: WE_AUX_CLEAR_UV_FLIP_FLAG_SOURCE,
        source_width,
        source_height,
        target_width,
        target_height,
        uv_y_flipped,
        uv_x_scale_bits,
        uv_y_scale_bits,
        clip_positions_bits: [
            [(-1.0f32).to_bits(), 1.0f32.to_bits(), 0.0f32.to_bits()],
            [(-1.0f32).to_bits(), (-3.0f32).to_bits(), 0.0f32.to_bits()],
            [3.0f32.to_bits(), 1.0f32.to_bits(), 0.0f32.to_bits()],
        ],
        uv_x_formula: ["0", "0", "2.0 * float(desc+0x2c) / float(desc+0x20)"],
        uv_y_normal_formula: [
            "float(desc+0x30) / float(desc+0x24)",
            "-float(desc+0x30) / float(desc+0x24)",
            "float(desc+0x30) / float(desc+0x24)",
        ],
        uv_y_flipped_formula: ["0", "2.0 * float(desc+0x2c) / float(desc+0x20)", "0"],
    }
}

fn push_aux_clear_vertex(bytes: &mut Vec<u8>, x: f32, y: f32, z: f32, u: f32, v: f32) {
    bytes.extend_from_slice(&x.to_le_bytes());
    bytes.extend_from_slice(&y.to_le_bytes());
    bytes.extend_from_slice(&z.to_le_bytes());
    bytes.extend_from_slice(&u.to_le_bytes());
    bytes.extend_from_slice(&v.to_le_bytes());
}

fn aux_material_draw_frame_order() -> [&'static str; 8] {
    [
        "read_aux_clear_prep_commands",
        "lower_aux_0x3f0_non_indexed_target_receiver",
        "lower_aux_0x3f8_indexed_target_receiver",
        "require_active_mdlv_geometry_residency",
        "preserve_0x14020a3ea_and_0x14020b1e8_call_arguments",
        "separate_aux_targets_from_mesh_geometry",
        "feed_layer_compositor_clear_prep_recorder",
        "keep_resource_heap_binding_model",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency, SceneLayerAuxCompositeTargetsResidency,
        SceneLayerCompositorEntry, SceneLayerCompositorOperation, SceneLayerCompositorRoute,
        WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT, WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
        WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR, WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_aux_clear_prep::native_vulkan_plan_scene_layer_aux_clear_prep;
    use crate::renderer::native_vulkan::scene_backend::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorRecordingBlockKind,
        NativeVulkanSceneLayerCompositorSchedulePlan, NativeVulkanSceneLayerCompositorScheduleStep,
        NativeVulkanSceneLayerCompositorScheduledKind,
    };

    #[test]
    fn aux_material_draw_plan_closes_3f0_and_3f8_receiver_contracts() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");

        let plan =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("aux material draw plan");

        assert_eq!(plan.active_block_count, 1);
        assert_eq!(plan.draw_receiver_count, 2);
        assert_eq!(plan.non_indexed_draw_receiver_count, 1);
        assert_eq!(plan.indexed_draw_receiver_count, 1);
        assert!(plan.covers_clear_prep(&clear_prep));

        let command = &plan.commands[0];
        assert_eq!(
            command.clear_material.receiver_kind,
            NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
        );
        assert_eq!(
            command.clear_material.wrapper_create_vma,
            WE_RT_TARGET_WRAPPER_CREATE_NON_INDEXED_VMA
        );
        assert_eq!(command.clear_material.vertex_count, 3);
        assert_eq!(command.clear_material.layout_bitmask, 0x9);
        assert!(command.clear_material.index_payload_source.is_none());
        let triangle = command
            .clear_material
            .clear_triangle_payload
            .expect("3f0 triangle payload");
        assert_eq!(triangle.position_constants_vma, [0x140492aa0, 0x140492af0]);
        assert_eq!(triangle.source_width, 3840);
        assert_eq!(triangle.source_height, 2160);
        assert_eq!(triangle.target_width, 3840);
        assert_eq!(triangle.target_height, 2160);
        assert_eq!(triangle.uv_x_scale_bits, 2.0f32.to_bits());
        assert_eq!(triangle.uv_y_scale_bits, 1.0f32.to_bits());
        assert_eq!(
            native_vulkan_scene_layer_aux_clear_triangle_payload(3840, 2160, 3840, 2160, false)
                .expect("triangle payload")
                .bytes
                .len(),
            WE_AUX_MATERIAL_CLEAR_TRIANGLE_PAYLOAD_BYTES
        );

        assert_eq!(
            command.generated_material.receiver_kind,
            NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
        );
        assert_eq!(
            command.generated_material.wrapper_create_vma,
            WE_RT_TARGET_WRAPPER_CREATE_INDEXED_VMA
        );
        assert_eq!(command.generated_material.index_width_selector, Some(0));
        assert_eq!(command.generated_material.topology_selector, 0);
        assert_eq!(command.generated_material.stack_usage_byte, Some(0));
        assert_eq!(command.generated_material.vertex_stride_bytes, 80);
        assert_eq!(command.generated_material.index_count, 23_988);
    }

    #[test]
    fn aux_material_draw_plan_rejects_missing_active_geometry() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let mut no_geometry = residency(object);
        no_geometry.resources.retain(|resource| {
            !matches!(
                resource,
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(_)
            )
        });

        let err = native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &no_geometry)
            .expect_err("active geometry is required");

        assert!(err.contains("[aux+0x3f8]"));
    }

    fn schedule(object: SceneObjectId) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        NativeVulkanSceneLayerCompositorSchedulePlan {
            layer_count: 1,
            command_count: 1,
            direct_mesh_graph_command_count: 0,
            object_final_producer_command_count: 0,
            object_final_composite_command_count: 0,
            alpha_mask_token_draw_list_command_count: 0,
            token_program_no_draw_count: 0,
            clear_prep_early_out_no_draw_count: 0,
            clear_prep_recorder_required_count: 1,
            recording_block_count: 1,
            mesh_graph_draw_span_block_count: 0,
            alpha_mask_token_recording_block_count: 0,
            no_draw_marker_block_count: 0,
            all_alpha_mask_commands_recordable: true,
            steps: vec![NativeVulkanSceneLayerCompositorScheduleStep {
                global_command_index: 0,
                layer_index: 0,
                layer_command_index: 0,
                object,
                route: SceneLayerCompositorRoute::DirectSwapchain,
                entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                operation: SceneLayerCompositorOperation::ClearPrep,
                scheduled_kind:
                    NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index: None,
                token_recording_step_index: None,
                command_order: vec!["classify_clear_prep_for_test"],
            }],
            recording_blocks: vec![NativeVulkanSceneLayerCompositorRecordingBlock {
                block_index: 0,
                step_index_start: 0,
                step_index_end: 1,
                command_count: 1,
                kind:
                    NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index_start: None,
                graph_draw_index_end: None,
                token_recording_step_index: None,
                command_order: vec!["record_clear_prep_for_test"],
            }],
            command_order: [
                "walk_layer_compositor_layers",
                "classify_layer_commands",
                "attach_mesh_graph_draw_indices",
                "attach_alpha_mask_token_recording_steps",
                "coalesce_no_draw_markers",
                "coalesce_contiguous_recording_blocks",
                "count_command_block_kinds",
                "emit_layer_compositor_schedule_plan",
            ],
        }
    }

    fn residency(object: SceneObjectId) -> SceneResourceResidencyPlan {
        SceneResourceResidencyPlan {
            resources: vec![
                SceneResidentResource::LayerAuxCompositeTargets(
                    SceneLayerAuxCompositeTargetsResidency {
                        object,
                        clear_target_3e8: true,
                        material_target_3f0: true,
                        effect_target_3f8: true,
                        generated_material_408: true,
                        clear_material_410: true,
                        clear_source_width: 3840,
                        clear_source_height: 2160,
                        clear_target_width: 3840,
                        clear_target_height: 2160,
                        clear_uv_y_flipped: false,
                        clear_target_color_format: 0,
                        clear_target_aux_format: WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
                        clear_target_r9_selector: WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
                        clear_target_resource_selector: WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
                        clear_target_cache_selector: WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
                        clear_prep_ready: true,
                    },
                ),
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(
                    SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency {
                        object,
                        entry_owner_index: 0,
                        layout_key: 0x180000f,
                        vertex_stride_bytes: 80,
                        vertex_count: 4106,
                        index_count: 23_988,
                        vertex_bytes: 328_480,
                        index_bytes: 47_976,
                        source_record_count: 44,
                        subdraw_count: 4,
                    },
                ),
            ],
        }
    }
}
