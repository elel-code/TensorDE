//! Target-like flattexture geometry facts for WE alpha-mask copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::vk;
use vulkanalia::vk::Handle;

pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY: &str =
    "target-like-flattexture";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT: &str =
    "a_Position.xyz+a_TexCoord.xy";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_HELPER_VMA: u64 = 0x1401ede30;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_LAYOUT_KEY_HELPER_VMA: u64 =
    0x140098c30;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_ATTRIBUTE_IDS: [u32; 2] =
    [0, 7];
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT_BITMASK: u32 = 0x9;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_POSITION_ATTRIBUTE_BYTES: u32 =
    12;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_TEXCOORD_ATTRIBUTE_BYTES: u32 =
    8;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_COUNT: u32 = 4;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES: u32 = 20;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES: u64 =
    FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES as u64 * FLATTEXTURE_COPY_BACK_VERTEX_COUNT as u64;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_COUNT: u32 = 6;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_ELEMENT_BYTES: u32 = 2;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_REQUIRED_INDEX_BYTES: u64 =
    FLATTEXTURE_COPY_BACK_INDEX_ELEMENT_BYTES as u64 * FLATTEXTURE_COPY_BACK_INDEX_COUNT as u64;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_FORMAT: &str =
    "DXGI_FORMAT_R16_UINT";
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_VK_INDEX_TYPE: vk::IndexType =
    vk::IndexType::UINT16;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_WIDTH_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_TOPOLOGY_SELECTOR: u32 = 0;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16: [u16; 6] =
    [0, 2, 1, 1, 2, 3];
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_BYTES: [u8; 12] =
    [0, 0, 2, 0, 1, 0, 1, 0, 2, 0, 3, 0];
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH: u64 =
    native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
        &FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_BYTES,
    );
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_CENTER_POSITION_FLAG: u32 = 0x1;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_TEXEL_INSET_FLAG: u32 = 0x2;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_FLIP_V_FLAG: u32 = 0x4;
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_HALF_EXTENT_BITS: u32 =
    0.5f32.to_bits();
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_TEXEL_INSET_BITS: u32 =
    0.15000000596046448f32.to_bits();
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_ONE_BITS: u32 = 1.0f32.to_bits();

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackTargetQuadInput
{
    pub position_extent: [f32; 2],
    pub texel_inset_denominator: [f32; 2],
    pub texcoord_extent: [f32; 2],
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackTargetVertexPayload
{
    pub bytes: [u8; FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES as usize],
    pub payload_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryBuffers
{
    pub vertex: vk::Buffer,
    pub index: vk::Buffer,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
    pub vertex_payload_hash: u64,
    pub index_payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryPlan
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

impl NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryPlan {
    pub(in crate::renderer::native_vulkan) fn from_raster_geometry_and_buffers(
        raster_geometry: &'static str,
        buffers: NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryBuffers,
    ) -> Result<Self, String> {
        if raster_geometry != FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY {
            return Err(format!(
                "scene layer alpha-mask copy-back command requires target-like-flattexture geometry, got {raster_geometry}"
            ));
        }
        if buffers.vertex == vk::Buffer::null() || buffers.index == vk::Buffer::null() {
            return Err(
                "scene layer alpha-mask copy-back command requires resident target-like flattexture vertex/index buffers"
                    .to_owned(),
            );
        }
        if buffers.vertex_bytes < FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES {
            return Err(format!(
                "scene layer alpha-mask copy-back target-like vertex buffer too small: {} bytes",
                buffers.vertex_bytes
            ));
        }
        if buffers.index_bytes < FLATTEXTURE_COPY_BACK_REQUIRED_INDEX_BYTES {
            return Err(format!(
                "scene layer alpha-mask copy-back target-like index buffer too small: {} bytes",
                buffers.index_bytes
            ));
        }
        if buffers.index_payload_hash != FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH {
            return Err(format!(
                "scene layer alpha-mask copy-back index payload hash mismatch: expected {:#x}, got {:#x}",
                FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH, buffers.index_payload_hash
            ));
        }
        Ok(Self {
            raster_geometry,
            helper_vma: FLATTEXTURE_COPY_BACK_HELPER_VMA,
            layout_key_helper_vma: FLATTEXTURE_COPY_BACK_LAYOUT_KEY_HELPER_VMA,
            vertex_layout: FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT,
            vertex_attribute_ids: FLATTEXTURE_COPY_BACK_VERTEX_ATTRIBUTE_IDS,
            vertex_layout_bitmask: FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT_BITMASK,
            vertex_count: FLATTEXTURE_COPY_BACK_VERTEX_COUNT,
            vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
            position_attribute_bytes: FLATTEXTURE_COPY_BACK_POSITION_ATTRIBUTE_BYTES,
            texcoord_attribute_bytes: FLATTEXTURE_COPY_BACK_TEXCOORD_ATTRIBUTE_BYTES,
            vertex_bytes: buffers.vertex_bytes,
            vertex_payload_hash: buffers.vertex_payload_hash,
            index_format: FLATTEXTURE_COPY_BACK_INDEX_FORMAT,
            index_width_selector: FLATTEXTURE_COPY_BACK_INDEX_WIDTH_SELECTOR,
            topology_selector: FLATTEXTURE_COPY_BACK_TOPOLOGY_SELECTOR,
            index_element_bytes: FLATTEXTURE_COPY_BACK_INDEX_ELEMENT_BYTES,
            index_count: FLATTEXTURE_COPY_BACK_INDEX_COUNT,
            index_bytes: buffers.index_bytes,
            index_payload_u16: FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16,
            index_payload_hash: buffers.index_payload_hash,
            center_position_flag: FLATTEXTURE_COPY_BACK_CENTER_POSITION_FLAG,
            texel_inset_flag: FLATTEXTURE_COPY_BACK_TEXEL_INSET_FLAG,
            flip_v_flag: FLATTEXTURE_COPY_BACK_FLIP_V_FLAG,
            half_extent_bits: FLATTEXTURE_COPY_BACK_HALF_EXTENT_BITS,
            texel_inset_bits: FLATTEXTURE_COPY_BACK_TEXEL_INSET_BITS,
            one_bits: FLATTEXTURE_COPY_BACK_ONE_BITS,
            command_order: [
                "cmd_bind_vertex_buffers",
                "cmd_bind_index_buffer_r16_uint",
                "cmd_draw_indexed",
            ],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_copy_back_vertex_payload_from_helper_inputs(
    input: NativeVulkanSceneLayerAlphaMaskCopyBackTargetQuadInput,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackTargetVertexPayload, String> {
    validate_finite_pair("position_extent", input.position_extent)?;
    validate_finite_pair("texel_inset_denominator", input.texel_inset_denominator)?;
    validate_finite_pair("texcoord_extent", input.texcoord_extent)?;

    let centered = input.flags & FLATTEXTURE_COPY_BACK_CENTER_POSITION_FLAG != 0;
    let texel_inset = input.flags & FLATTEXTURE_COPY_BACK_TEXEL_INSET_FLAG != 0;
    let flip_v = input.flags & FLATTEXTURE_COPY_BACK_FLIP_V_FLAG != 0;

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
                "scene layer alpha-mask copy-back target-like vertex helper requires non-zero texel inset denominators when flag 0x2 is set"
                    .to_owned(),
            );
        }
        (
            f32::from_bits(FLATTEXTURE_COPY_BACK_TEXEL_INSET_BITS)
                / input.texel_inset_denominator[0],
            f32::from_bits(FLATTEXTURE_COPY_BACK_TEXEL_INSET_BITS)
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
    let mut bytes = [0u8; FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES as usize];
    for (vertex_index, vertex) in vertices.iter().enumerate() {
        let base = vertex_index * FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES as usize;
        for (component_index, component) in vertex.iter().enumerate() {
            let offset = base + component_index * std::mem::size_of::<f32>();
            bytes[offset..offset + std::mem::size_of::<f32>()]
                .copy_from_slice(&component.to_le_bytes());
        }
    }

    Ok(NativeVulkanSceneLayerAlphaMaskCopyBackTargetVertexPayload {
        bytes,
        payload_hash: native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(&bytes),
    })
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
                "scene layer alpha-mask copy-back target-like vertex helper got non-finite {label}[{index}]"
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
    fn copy_back_geometry_models_recovered_layout_and_r16_index_payload() {
        assert_eq!(FLATTEXTURE_COPY_BACK_HELPER_VMA, 0x1401ede30);
        assert_eq!(FLATTEXTURE_COPY_BACK_LAYOUT_KEY_HELPER_VMA, 0x140098c30);
        assert_eq!(FLATTEXTURE_COPY_BACK_VERTEX_ATTRIBUTE_IDS, [0, 7]);
        assert_eq!(FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT_BITMASK, 0x9);
        assert_eq!(FLATTEXTURE_COPY_BACK_POSITION_ATTRIBUTE_BYTES, 12);
        assert_eq!(FLATTEXTURE_COPY_BACK_TEXCOORD_ATTRIBUTE_BYTES, 8);
        assert_eq!(FLATTEXTURE_COPY_BACK_VERTEX_COUNT, 4);
        assert_eq!(FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES, 20);
        assert_eq!(FLATTEXTURE_COPY_BACK_INDEX_COUNT, 6);
        assert_eq!(FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES, 80);
        assert_eq!(FLATTEXTURE_COPY_BACK_REQUIRED_INDEX_BYTES, 12);
        assert_eq!(FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16, [0, 2, 1, 1, 2, 3]);
        assert_eq!(FLATTEXTURE_COPY_BACK_VK_INDEX_TYPE, vk::IndexType::UINT16);
        assert_eq!(FLATTEXTURE_COPY_BACK_INDEX_WIDTH_SELECTOR, 0);
        assert_eq!(FLATTEXTURE_COPY_BACK_TOPOLOGY_SELECTOR, 0);
        assert_eq!(
            FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH,
            native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
                &FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_BYTES,
            )
        );
    }

    #[test]
    fn copy_back_geometry_plan_rejects_wrong_index_payload() {
        let mut buffers = geometry_buffers();
        buffers.index_payload_hash = 0;

        let err =
            NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryPlan::from_raster_geometry_and_buffers(
                FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
                buffers,
            )
            .expect_err("wrong index payload hash must fail");

        assert!(err.contains("index payload hash mismatch"));
    }

    #[test]
    fn copy_back_geometry_plan_reports_helper_layout_facts() {
        let plan =
            NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryPlan::from_raster_geometry_and_buffers(
                FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
                geometry_buffers(),
            )
            .expect("copy-back geometry plan");

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
    fn copy_back_vertex_payload_matches_recovered_helper_formula_without_flags() {
        let payload =
            native_vulkan_scene_layer_alpha_mask_copy_back_vertex_payload_from_helper_inputs(
                NativeVulkanSceneLayerAlphaMaskCopyBackTargetQuadInput {
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
    fn copy_back_vertex_payload_matches_center_inset_and_v_flip_flags() {
        let payload =
            native_vulkan_scene_layer_alpha_mask_copy_back_vertex_payload_from_helper_inputs(
                NativeVulkanSceneLayerAlphaMaskCopyBackTargetQuadInput {
                    position_extent: [10.0, 20.0],
                    texel_inset_denominator: [100.0, 50.0],
                    texcoord_extent: [0.8, 0.6],
                    flags: FLATTEXTURE_COPY_BACK_CENTER_POSITION_FLAG
                        | FLATTEXTURE_COPY_BACK_TEXEL_INSET_FLAG
                        | FLATTEXTURE_COPY_BACK_FLIP_V_FLAG,
                },
            )
            .expect("target-like vertex payload");

        let texel = f32::from_bits(FLATTEXTURE_COPY_BACK_TEXEL_INSET_BITS);
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
    fn copy_back_vertex_payload_rejects_zero_texel_denominator_when_inset_enabled() {
        let err = native_vulkan_scene_layer_alpha_mask_copy_back_vertex_payload_from_helper_inputs(
            NativeVulkanSceneLayerAlphaMaskCopyBackTargetQuadInput {
                position_extent: [1.0, 1.0],
                texel_inset_denominator: [0.0, 1.0],
                texcoord_extent: [1.0, 1.0],
                flags: FLATTEXTURE_COPY_BACK_TEXEL_INSET_FLAG,
            },
        )
        .expect_err("zero denominator must fail");

        assert!(err.contains("non-zero texel inset denominators"));
    }

    fn geometry_buffers() -> NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryBuffers {
        NativeVulkanSceneLayerAlphaMaskCopyBackTargetGeometryBuffers {
            vertex: vk::Buffer::from_raw(11),
            index: vk::Buffer::from_raw(12),
            vertex_bytes: FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES,
            index_bytes: FLATTEXTURE_COPY_BACK_REQUIRED_INDEX_BYTES,
            vertex_payload_hash: 100,
            index_payload_hash: FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH,
        }
    }

    fn payload_f32s(
        payload: &[u8; FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES as usize],
    ) -> [f32; 20] {
        let mut values = [0.0f32; 20];
        for (index, chunk) in payload.chunks_exact(std::mem::size_of::<f32>()).enumerate() {
            values[index] = f32::from_le_bytes(chunk.try_into().expect("f32 bytes"));
        }
        values
    }
}
