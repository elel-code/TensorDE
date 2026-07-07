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
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16: [u16; 6] =
    [0, 2, 1, 1, 2, 3];
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_BYTES: [u8; 12] =
    [0, 0, 2, 0, 1, 0, 1, 0, 2, 0, 3, 0];
pub(in crate::renderer::native_vulkan) const FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_HASH: u64 =
    native_vulkan_scene_layer_alpha_mask_copy_back_payload_hash(
        &FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_BYTES,
    );

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

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
    pub vertex_layout: &'static str,
    pub vertex_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_bytes: u64,
    pub vertex_payload_hash: u64,
    pub index_format: &'static str,
    pub index_element_bytes: u32,
    pub index_count: u32,
    pub index_bytes: u64,
    pub index_payload_u16: [u16; 6],
    pub index_payload_hash: u64,
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
            vertex_layout: FLATTEXTURE_COPY_BACK_VERTEX_LAYOUT,
            vertex_count: FLATTEXTURE_COPY_BACK_VERTEX_COUNT,
            vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
            vertex_bytes: buffers.vertex_bytes,
            vertex_payload_hash: buffers.vertex_payload_hash,
            index_format: FLATTEXTURE_COPY_BACK_INDEX_FORMAT,
            index_element_bytes: FLATTEXTURE_COPY_BACK_INDEX_ELEMENT_BYTES,
            index_count: FLATTEXTURE_COPY_BACK_INDEX_COUNT,
            index_bytes: buffers.index_bytes,
            index_payload_u16: FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16,
            index_payload_hash: buffers.index_payload_hash,
            command_order: [
                "cmd_bind_vertex_buffers",
                "cmd_bind_index_buffer_r16_uint",
                "cmd_draw_indexed",
            ],
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn copy_back_geometry_models_recovered_r16_index_payload() {
        assert_eq!(FLATTEXTURE_COPY_BACK_VERTEX_COUNT, 4);
        assert_eq!(FLATTEXTURE_COPY_BACK_INDEX_COUNT, 6);
        assert_eq!(FLATTEXTURE_COPY_BACK_REQUIRED_VERTEX_BYTES, 80);
        assert_eq!(FLATTEXTURE_COPY_BACK_REQUIRED_INDEX_BYTES, 12);
        assert_eq!(FLATTEXTURE_COPY_BACK_INDEX_PAYLOAD_U16, [0, 2, 1, 1, 2, 3]);
        assert_eq!(FLATTEXTURE_COPY_BACK_VK_INDEX_TYPE, vk::IndexType::UINT16);
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
}
