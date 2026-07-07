//! Geometry facts for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::vk;
use vulkanalia::vk::Handle;

pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY: &str =
    "render-state-flattexture-copy-back";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_SOURCE_FIELD: &str =
    "render_state+0x48";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_SOURCE_FIELD_OFFSET: u32 = 0x48;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_RENDER_STATE_CTOR_VMA: u64 =
    0x14017c6d0;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_FIELD_NULL_INIT_VMA: u64 =
    0x14017c73f;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_DRAW_LOAD_VMA: u64 = 0x14020da78;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_DRAW_CALL_VMA: u64 = 0x14020da7f;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_BLEND_TOGGLE_VMA: u64 =
    0x14020da40;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_BLEND_KEY_BIT: u32 = 0x100;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_FULL_ALPHA_MASK_OFFSET: u32 =
    0x1500;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INTERMEDIATE_OFFSET: u32 =
    0x1508;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_WRAPPER_OFFSET: u32 = 0x1518;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_WRAPPER_STORE_VMA: u64 =
    0x14017d01e;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT: &str =
    "a_Position.xyz+a_TexCoord.xy";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES: u32 = 20;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_MIN_VERTEX_COUNT: u32 = 3;

pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY: &str =
    "target-like-indexed-quad-helper";
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_HELPER_VMA: u64 = 0x1401ede30;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_LAYOUT_KEY_HELPER_VMA: u64 =
    0x140098c30;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_VERTEX_ATTRIBUTE_IDS: [u32;
    2] = [0, 7];
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_VERTEX_LAYOUT_BITMASK: u32 =
    0x9;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_POSITION_ATTRIBUTE_BYTES:
    u32 = 12;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_TEXCOORD_ATTRIBUTE_BYTES:
    u32 = 8;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_VERTEX_COUNT: u32 = 4;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_VERTEX_STRIDE_BYTES: u32 = 20;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES: u64 =
    TARGET_LIKE_INDEXED_QUAD_VERTEX_STRIDE_BYTES as u64
        * TARGET_LIKE_INDEXED_QUAD_VERTEX_COUNT as u64;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_COUNT: u32 = 6;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_ELEMENT_BYTES: u32 = 2;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_REQUIRED_INDEX_BYTES: u64 =
    TARGET_LIKE_INDEXED_QUAD_INDEX_ELEMENT_BYTES as u64
        * TARGET_LIKE_INDEXED_QUAD_INDEX_COUNT as u64;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_FORMAT: &str =
    "DXGI_FORMAT_R16_UINT";
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_WIDTH_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_TOPOLOGY_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_U16: [u16; 6] =
    [0, 2, 1, 1, 2, 3];
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_BYTES: [u8;
    12] = [0, 0, 2, 0, 1, 0, 1, 0, 2, 0, 3, 0];
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_HASH: u64 =
    native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
        &TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_BYTES,
    );
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_CENTER_POSITION_FLAG: u32 =
    0x1;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_FLAG: u32 = 0x2;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_FLIP_V_FLAG: u32 = 0x4;
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_HALF_EXTENT_BITS: u32 =
    0.5f32.to_bits();
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_BITS: u32 =
    0.15000000596046448f32.to_bits();
pub(in crate::renderer::native_vulkan) const TARGET_LIKE_INDEXED_QUAD_ONE_BITS: u32 =
    1.0f32.to_bits();

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers
{
    pub vertex: vk::Buffer,
    pub vertex_bytes: u64,
    pub vertex_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan
{
    pub raster_geometry: &'static str,
    pub source_field: &'static str,
    pub source_field_offset: u32,
    pub render_state_ctor_vma: u64,
    pub field_null_init_vma: u64,
    pub draw_load_vma: u64,
    pub draw_call_vma: u64,
    pub blend_toggle_vma: u64,
    pub blend_key_bit: u32,
    pub full_alpha_mask_offset: u32,
    pub intermediate_offset: u32,
    pub wrapper_offset: u32,
    pub wrapper_store_vma: u64,
    pub vertex_layout: &'static str,
    pub vertex_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_bytes: u64,
    pub vertex_payload_hash: u64,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 2],
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan {
    pub(in crate::renderer::native_vulkan) fn from_raster_geometry_and_buffers(
        raster_geometry: &'static str,
        buffers: NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
    ) -> Result<Self, String> {
        if raster_geometry != FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY {
            return Err(format!(
                "scene layer alpha-mask copy-back command requires render-state-flattexture-copy-back geometry, got {raster_geometry}"
            ));
        }
        if buffers.vertex == vk::Buffer::null() {
            return Err(
                "scene layer alpha-mask copy-back command requires resident render-state flattexture vertex buffer from render_state+0x48"
                    .to_owned(),
            );
        }
        if buffers.vertex_count < FLATTEXTURE_COPY_BACK_MIN_VERTEX_COUNT {
            return Err(format!(
                "scene layer alpha-mask copy-back render-state geometry has too few vertices: {}",
                buffers.vertex_count
            ));
        }
        if buffers.vertex_stride_bytes != FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES {
            return Err(format!(
                "scene layer alpha-mask copy-back render-state geometry requires flattexture stride {} bytes, got {}",
                FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES, buffers.vertex_stride_bytes
            ));
        }
        let required_vertex_bytes =
            u64::from(buffers.vertex_count) * u64::from(buffers.vertex_stride_bytes);
        if buffers.vertex_bytes < required_vertex_bytes {
            return Err(format!(
                "scene layer alpha-mask copy-back render-state vertex buffer too small: {} bytes for {} vertices at {} bytes",
                buffers.vertex_bytes, buffers.vertex_count, buffers.vertex_stride_bytes
            ));
        }
        Ok(Self {
            raster_geometry,
            source_field: FLATTEXTURE_COPY_BACK_SOURCE_FIELD,
            source_field_offset: FLATTEXTURE_COPY_BACK_SOURCE_FIELD_OFFSET,
            render_state_ctor_vma: FLATTEXTURE_COPY_BACK_RENDER_STATE_CTOR_VMA,
            field_null_init_vma: FLATTEXTURE_COPY_BACK_FIELD_NULL_INIT_VMA,
            draw_load_vma: FLATTEXTURE_COPY_BACK_DRAW_LOAD_VMA,
            draw_call_vma: FLATTEXTURE_COPY_BACK_DRAW_CALL_VMA,
            blend_toggle_vma: FLATTEXTURE_COPY_BACK_BLEND_TOGGLE_VMA,
            blend_key_bit: FLATTEXTURE_COPY_BACK_BLEND_KEY_BIT,
            full_alpha_mask_offset: FLATTEXTURE_COPY_BACK_FULL_ALPHA_MASK_OFFSET,
            intermediate_offset: FLATTEXTURE_COPY_BACK_INTERMEDIATE_OFFSET,
            wrapper_offset: FLATTEXTURE_COPY_BACK_WRAPPER_OFFSET,
            wrapper_store_vma: FLATTEXTURE_COPY_BACK_WRAPPER_STORE_VMA,
            vertex_layout: FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT,
            vertex_count: buffers.vertex_count,
            vertex_stride_bytes: buffers.vertex_stride_bytes,
            vertex_bytes: buffers.vertex_bytes,
            vertex_payload_hash: buffers.vertex_payload_hash,
            draw_call: "vkCmdDraw",
            command_order: [
                "cmd_bind_render_state_flattexture_copy_back_vertex_buffer",
                "cmd_draw_render_state_flattexture_copy_back",
            ],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadInput
{
    pub position_extent: [f32; 2],
    pub texel_inset_denominator: [f32; 2],
    pub texcoord_extent: [f32; 2],
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadVertexPayload
{
    pub bytes: [u8; TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES as usize],
    pub payload_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadBuffers
{
    pub vertex: vk::Buffer,
    pub index: vk::Buffer,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub vertex_payload_hash: u64,
    pub index_payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadPlan
{
    pub raster_geometry: &'static str,
    pub helper_vma: u64,
    pub layout_key_helper_vma: u64,
    pub vertex_layout: &'static str,
    pub vertex_attribute_ids: [u32; 2],
    pub vertex_layout_bitmask: u32,
    pub vertex_count: u32,
    pub vertex_stride_bytes: u32,
    pub position_attribute_bytes: u32,
    pub texcoord_attribute_bytes: u32,
    pub vertex_bytes: u64,
    pub vertex_payload_hash: u64,
    pub index_format: &'static str,
    pub index_width_selector: u32,
    pub topology_selector: u32,
    pub index_element_bytes: u32,
    pub index_count: u32,
    pub index_bytes: u64,
    pub index_payload_u16: [u16; 6],
    pub index_payload_hash: u64,
    pub center_position_flag: u32,
    pub texel_inset_flag: u32,
    pub flip_v_flag: u32,
    pub half_extent_bits: u32,
    pub texel_inset_bits: u32,
    pub one_bits: u32,
    pub command_order: [&'static str; 3],
}

impl NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadPlan {
    pub(in crate::renderer::native_vulkan) fn from_raster_geometry_and_buffers(
        raster_geometry: &'static str,
        buffers: NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadBuffers,
    ) -> Result<Self, String> {
        if raster_geometry != TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY {
            return Err(format!(
                "scene layer alpha-mask target-like helper requires target-like-indexed-quad-helper geometry, got {raster_geometry}"
            ));
        }
        if buffers.vertex == vk::Buffer::null() || buffers.index == vk::Buffer::null() {
            return Err(
                "scene layer alpha-mask target-like helper requires resident vertex/index buffers"
                    .to_owned(),
            );
        }
        if buffers.vertex_bytes < TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES {
            return Err(format!(
                "scene layer alpha-mask target-like helper vertex buffer too small: {} bytes",
                buffers.vertex_bytes
            ));
        }
        if buffers.index_bytes < TARGET_LIKE_INDEXED_QUAD_REQUIRED_INDEX_BYTES {
            return Err(format!(
                "scene layer alpha-mask target-like helper index buffer too small: {} bytes",
                buffers.index_bytes
            ));
        }
        if buffers.index_payload_hash != TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_HASH {
            return Err(format!(
                "scene layer alpha-mask target-like helper index payload hash mismatch: expected {:#x}, got {:#x}",
                TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_HASH, buffers.index_payload_hash
            ));
        }
        Ok(Self {
            raster_geometry,
            helper_vma: TARGET_LIKE_INDEXED_QUAD_HELPER_VMA,
            layout_key_helper_vma: TARGET_LIKE_INDEXED_QUAD_LAYOUT_KEY_HELPER_VMA,
            vertex_layout: FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT,
            vertex_attribute_ids: TARGET_LIKE_INDEXED_QUAD_VERTEX_ATTRIBUTE_IDS,
            vertex_layout_bitmask: TARGET_LIKE_INDEXED_QUAD_VERTEX_LAYOUT_BITMASK,
            vertex_count: TARGET_LIKE_INDEXED_QUAD_VERTEX_COUNT,
            vertex_stride_bytes: TARGET_LIKE_INDEXED_QUAD_VERTEX_STRIDE_BYTES,
            position_attribute_bytes: TARGET_LIKE_INDEXED_QUAD_POSITION_ATTRIBUTE_BYTES,
            texcoord_attribute_bytes: TARGET_LIKE_INDEXED_QUAD_TEXCOORD_ATTRIBUTE_BYTES,
            vertex_bytes: buffers.vertex_bytes,
            vertex_payload_hash: buffers.vertex_payload_hash,
            index_format: TARGET_LIKE_INDEXED_QUAD_INDEX_FORMAT,
            index_width_selector: TARGET_LIKE_INDEXED_QUAD_INDEX_WIDTH_SELECTOR,
            topology_selector: TARGET_LIKE_INDEXED_QUAD_TOPOLOGY_SELECTOR,
            index_element_bytes: TARGET_LIKE_INDEXED_QUAD_INDEX_ELEMENT_BYTES,
            index_count: TARGET_LIKE_INDEXED_QUAD_INDEX_COUNT,
            index_bytes: buffers.index_bytes,
            index_payload_u16: TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_U16,
            index_payload_hash: buffers.index_payload_hash,
            center_position_flag: TARGET_LIKE_INDEXED_QUAD_CENTER_POSITION_FLAG,
            texel_inset_flag: TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_FLAG,
            flip_v_flag: TARGET_LIKE_INDEXED_QUAD_FLIP_V_FLAG,
            half_extent_bits: TARGET_LIKE_INDEXED_QUAD_HALF_EXTENT_BITS,
            texel_inset_bits: TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_BITS,
            one_bits: TARGET_LIKE_INDEXED_QUAD_ONE_BITS,
            command_order: [
                "wrapper_plus_0x40_create_indexed_draw_target",
                "cmd_bind_indexed_target_like_vertex_index_buffers",
                "cmd_draw_indexed_target_like_helper",
            ],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_target_like_indexed_quad_payload_from_helper_inputs(
    input: NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadInput,
) -> Result<NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadVertexPayload, String> {
    validate_finite_pair("position_extent", input.position_extent)?;
    validate_finite_pair("texel_inset_denominator", input.texel_inset_denominator)?;
    validate_finite_pair("texcoord_extent", input.texcoord_extent)?;

    let centered = input.flags & TARGET_LIKE_INDEXED_QUAD_CENTER_POSITION_FLAG != 0;
    let texel_inset = input.flags & TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_FLAG != 0;
    let flip_v = input.flags & TARGET_LIKE_INDEXED_QUAD_FLIP_V_FLAG != 0;

    let (left, right, bottom, top) = if centered {
        let half_x = input.position_extent[0] * 0.5;
        let half_y = input.position_extent[1] * 0.5;
        (-half_x, half_x, -half_y, half_y)
    } else {
        (0.0, input.position_extent[0], 0.0, input.position_extent[1])
    };

    let (u0, v0_inset) = if texel_inset {
        if input.texel_inset_denominator[0] == 0.0 || input.texel_inset_denominator[1] == 0.0 {
            return Err(
                "scene layer alpha-mask target-like helper requires non-zero texel inset denominators when flag 0x2 is set"
                    .to_owned(),
            );
        }
        (
            f32::from_bits(TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_BITS)
                / input.texel_inset_denominator[0],
            f32::from_bits(TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_BITS)
                / input.texel_inset_denominator[1],
        )
    } else {
        (0.0, 0.0)
    };
    let u1 = input.texcoord_extent[0] - u0;
    let v1_before_flip = input.texcoord_extent[1] - v0_inset;
    let (v0, v1) = if flip_v {
        (1.0, 1.0 - v1_before_flip)
    } else {
        (v0_inset, v1_before_flip)
    };

    let vertices = [
        [left, top, 0.0, u0, v0],
        [right, top, 0.0, u1, v0],
        [left, bottom, 0.0, u0, v1],
        [right, bottom, 0.0, u1, v1],
    ];
    let mut bytes = [0u8; TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES as usize];
    for (vertex_index, vertex) in vertices.iter().enumerate() {
        let base = vertex_index * TARGET_LIKE_INDEXED_QUAD_VERTEX_STRIDE_BYTES as usize;
        for (component_index, component) in vertex.iter().enumerate() {
            let offset = base + component_index * std::mem::size_of::<f32>();
            bytes[offset..offset + std::mem::size_of::<f32>()]
                .copy_from_slice(&component.to_le_bytes());
        }
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadVertexPayload {
            bytes,
            payload_hash: native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(&bytes),
        },
    )
}

pub(in crate::renderer::native_vulkan) const fn native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
    payload: &[u8],
) -> u64 {
    let mut hash = FNV1A64_OFFSET;
    let mut index = 0;
    while index < payload.len() {
        hash ^= payload[index] as u64;
        hash = hash.wrapping_mul(FNV1A64_PRIME);
        index += 1;
    }
    hash
}

fn validate_finite_pair(label: &'static str, values: [f32; 2]) -> Result<(), String> {
    for (index, value) in values.into_iter().enumerate() {
        if !value.is_finite() {
            return Err(format!(
                "scene layer alpha-mask target-like helper got non-finite {label}[{index}]"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn copy_back_geometry_models_render_state_shared_draw_object() {
        let plan =
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
                FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
                render_state_geometry_buffers(),
            )
            .expect("copy-back render-state geometry plan");

        assert_eq!(plan.raster_geometry, "render-state-flattexture-copy-back");
        assert_eq!(plan.source_field, "render_state+0x48");
        assert_eq!(plan.source_field_offset, 0x48);
        assert_eq!(plan.render_state_ctor_vma, 0x14017c6d0);
        assert_eq!(plan.field_null_init_vma, 0x14017c73f);
        assert_eq!(plan.draw_load_vma, 0x14020da78);
        assert_eq!(plan.draw_call_vma, 0x14020da7f);
        assert_eq!(plan.blend_toggle_vma, 0x14020da40);
        assert_eq!(plan.blend_key_bit, 0x100);
        assert_eq!(plan.full_alpha_mask_offset, 0x1500);
        assert_eq!(plan.intermediate_offset, 0x1508);
        assert_eq!(plan.wrapper_offset, 0x1518);
        assert_eq!(plan.wrapper_store_vma, 0x14017d01e);
        assert_eq!(plan.vertex_layout, "a_Position.xyz+a_TexCoord.xy");
        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(plan.draw_call, "vkCmdDraw");
        assert_eq!(
            plan.command_order,
            [
                "cmd_bind_render_state_flattexture_copy_back_vertex_buffer",
                "cmd_draw_render_state_flattexture_copy_back"
            ]
        );
    }

    #[test]
    fn copy_back_geometry_rejects_indexed_quad_helper_as_runtime_copy_back() {
        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
                TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY,
                render_state_geometry_buffers(),
            )
            .expect_err("indexed helper must not be accepted as copy-back draw");

        assert!(err.contains("requires render-state-flattexture-copy-back geometry"));
    }

    #[test]
    fn copy_back_geometry_requires_resident_render_state_vertex_buffer() {
        let mut buffers = render_state_geometry_buffers();
        buffers.vertex = vk::Buffer::null();

        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
                FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
                buffers,
            )
            .expect_err("missing render-state vertex buffer must fail");

        assert!(err.contains("render_state+0x48"));
    }

    #[test]
    fn copy_back_geometry_rejects_non_flattexture_stride() {
        let mut buffers = render_state_geometry_buffers();
        buffers.vertex_stride_bytes = 16;

        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
                FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
                buffers,
            )
            .expect_err("wrong stride must fail");

        assert!(err.contains("requires flattexture stride 20 bytes"));
    }

    #[test]
    fn target_like_helper_models_recovered_layout_and_r16_index_payload() {
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_HELPER_VMA, 0x1401ede30);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_LAYOUT_KEY_HELPER_VMA, 0x140098c30);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_VERTEX_ATTRIBUTE_IDS, [0, 7]);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_VERTEX_LAYOUT_BITMASK, 0x9);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_POSITION_ATTRIBUTE_BYTES, 12);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_TEXCOORD_ATTRIBUTE_BYTES, 8);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_VERTEX_COUNT, 4);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_VERTEX_STRIDE_BYTES, 20);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_INDEX_COUNT, 6);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES, 80);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_REQUIRED_INDEX_BYTES, 12);
        assert_eq!(
            TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_U16,
            [0, 2, 1, 1, 2, 3]
        );
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_INDEX_WIDTH_SELECTOR, 0);
        assert_eq!(TARGET_LIKE_INDEXED_QUAD_TOPOLOGY_SELECTOR, 0);
        assert_eq!(
            TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_HASH,
            native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
                &TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_BYTES,
            )
        );
    }

    #[test]
    fn target_like_helper_plan_rejects_wrong_index_payload() {
        let mut buffers = target_like_helper_buffers();
        buffers.index_payload_hash = 0;

        let err =
            NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadPlan::from_raster_geometry_and_buffers(
                TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY,
                buffers,
            )
            .expect_err("wrong index payload hash must fail");

        assert!(err.contains("index payload hash mismatch"));
    }

    #[test]
    fn target_like_helper_plan_reports_helper_layout_facts() {
        let plan =
            NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadPlan::from_raster_geometry_and_buffers(
                TARGET_LIKE_INDEXED_QUAD_HELPER_RASTER_GEOMETRY,
                target_like_helper_buffers(),
            )
            .expect("target-like helper geometry plan");

        assert_eq!(plan.helper_vma, 0x1401ede30);
        assert_eq!(plan.layout_key_helper_vma, 0x140098c30);
        assert_eq!(plan.vertex_attribute_ids, [0, 7]);
        assert_eq!(plan.vertex_layout_bitmask, 0x9);
        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(plan.index_width_selector, 0);
        assert_eq!(plan.topology_selector, 0);
        assert_eq!(plan.texel_inset_bits, 0.15000000596046448f32.to_bits());
        assert_eq!(plan.one_bits, 1.0f32.to_bits());
    }

    #[test]
    fn target_like_helper_vertex_payload_matches_recovered_formula_without_flags() {
        let payload =
            native_vulkan_scene_layer_alpha_mask_target_like_indexed_quad_payload_from_helper_inputs(
                NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadInput {
                    position_extent: [2.0, 4.0],
                    texel_inset_denominator: [64.0, 128.0],
                    texcoord_extent: [1.0, 1.0],
                    flags: 0,
                },
            )
            .expect("target-like vertex payload");

        assert_eq!(
            payload_f32s(&payload.bytes),
            [
                0.0, 4.0, 0.0, 0.0, 0.0, 2.0, 4.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0,
                0.0, 0.0, 1.0, 1.0,
            ]
        );
        assert_eq!(
            payload.payload_hash,
            native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(&payload.bytes)
        );
    }

    #[test]
    fn target_like_helper_vertex_payload_matches_center_inset_and_v_flip_flags() {
        let payload =
            native_vulkan_scene_layer_alpha_mask_target_like_indexed_quad_payload_from_helper_inputs(
                NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadInput {
                    position_extent: [10.0, 20.0],
                    texel_inset_denominator: [100.0, 50.0],
                    texcoord_extent: [0.8, 0.6],
                    flags: TARGET_LIKE_INDEXED_QUAD_CENTER_POSITION_FLAG
                        | TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_FLAG
                        | TARGET_LIKE_INDEXED_QUAD_FLIP_V_FLAG,
                },
            )
            .expect("target-like vertex payload");

        let texel = f32::from_bits(TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_BITS);
        let u0 = texel / 100.0;
        let u1 = 0.8 - u0;
        let v1 = 1.0 - (0.6 - texel / 50.0);
        assert_eq!(
            payload_f32s(&payload.bytes),
            [
                -5.0, 10.0, 0.0, u0, 1.0, 5.0, 10.0, 0.0, u1, 1.0, -5.0, -10.0, 0.0, u0, v1, 5.0,
                -10.0, 0.0, u1, v1,
            ]
        );
    }

    #[test]
    fn target_like_helper_vertex_payload_rejects_zero_texel_denominator_when_inset_enabled() {
        let err =
            native_vulkan_scene_layer_alpha_mask_target_like_indexed_quad_payload_from_helper_inputs(
                NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadInput {
                    position_extent: [1.0, 1.0],
                    texel_inset_denominator: [0.0, 1.0],
                    texcoord_extent: [1.0, 1.0],
                    flags: TARGET_LIKE_INDEXED_QUAD_TEXEL_INSET_FLAG,
                },
            )
            .expect_err("zero denominator must fail");

        assert!(err.contains("non-zero texel inset denominators"));
    }

    fn render_state_geometry_buffers()
    -> NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
            vertex: vk::Buffer::from_raw(11),
            vertex_bytes: 80,
            vertex_count: 4,
            vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
            vertex_payload_hash: 100,
        }
    }

    fn target_like_helper_buffers() -> NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadBuffers {
        NativeVulkanSceneLayerAlphaMaskTargetLikeIndexedQuadBuffers {
            vertex: vk::Buffer::from_raw(11),
            index: vk::Buffer::from_raw(12),
            vertex_bytes: TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES,
            index_bytes: TARGET_LIKE_INDEXED_QUAD_REQUIRED_INDEX_BYTES,
            vertex_payload_hash: 100,
            index_payload_hash: TARGET_LIKE_INDEXED_QUAD_INDEX_PAYLOAD_HASH,
        }
    }

    fn payload_f32s(
        payload: &[u8; TARGET_LIKE_INDEXED_QUAD_REQUIRED_VERTEX_BYTES as usize],
    ) -> [f32; 20] {
        let mut values = [0.0f32; 20];
        for (index, chunk) in payload.chunks_exact(std::mem::size_of::<f32>()).enumerate() {
            values[index] = f32::from_le_bytes(chunk.try_into().expect("f32 bytes"));
        }
        values
    }
}
