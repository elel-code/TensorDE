//! Scene texture descriptor heap frame plan.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk::{self, Handle};

use crate::engine::scene_engine::{
    SceneGraphResourceRole, SceneObjectId, SceneResourceId, SceneTextureFormat,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput,
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan,
};

use super::super::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorBinding, NativeVulkanSceneTextureDescriptorFramePlan,
};
use super::super::texture_images::NativeVulkanSceneTextureImageBinding;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapFramePlan {
    pub draw_binding_count: usize,
    pub texture_count: usize,
    pub descriptor_model: &'static str,
    pub entries: Vec<NativeVulkanSceneTextureHeapEntry>,
    pub draw_bindings: Vec<NativeVulkanSceneTextureHeapDrawBinding>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pub command_order: [&'static str; 4],
    #[serde(skip)]
    pub(super) bindings: Vec<NativeVulkanSceneTextureHeapImageBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapEntry {
    pub resource: SceneResourceId,
    pub heap_index: usize,
    pub image_handle: u64,
    pub view_handle: u64,
    pub sampler_handle: u64,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub shader_mapping: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapDrawBinding {
    pub object: SceneObjectId,
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub resource: SceneResourceId,
    pub heap_index: usize,
    pub shader_mapping: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanSceneTextureHeapImageBinding {
    pub(super) resource: SceneResourceId,
    pub(super) heap_index: usize,
    pub(super) image: vk::Image,
    pub(super) format: vk::Format,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) mip_count: u32,
}

impl NativeVulkanSceneTextureHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_texture_descriptors<ResolveBinding>(
        descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        mut resolve_binding: ResolveBinding,
    ) -> Result<Self, String>
    where
        ResolveBinding:
            FnMut(SceneResourceId) -> Result<NativeVulkanSceneTextureImageBinding, String>,
    {
        let mut resource_to_heap_index = BTreeMap::new();
        let mut entries = Vec::new();
        let mut bindings = Vec::new();
        let mut draw_bindings = Vec::with_capacity(descriptors.bindings.len());

        for descriptor in &descriptors.bindings {
            let heap_index = if let Some(heap_index) =
                resource_to_heap_index.get(&descriptor.resource).copied()
            {
                heap_index
            } else {
                let heap_index = entries.len();
                let binding = resolve_binding(descriptor.resource)?;
                let image_binding = heap_image_binding(descriptor, binding, heap_index)?;
                let entry = heap_entry(descriptor, binding, heap_index);
                resource_to_heap_index.insert(descriptor.resource, heap_index);
                entries.push(entry);
                bindings.push(image_binding);
                heap_index
            };
            draw_bindings.push(NativeVulkanSceneTextureHeapDrawBinding {
                object: descriptor.object,
                slot: descriptor.slot,
                role: descriptor.role,
                resource: descriptor.resource,
                heap_index,
                shader_mapping: descriptor.shader_mapping,
            });
        }

        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: entries.len(),
                properties: descriptor_heap_properties,
            },
        );
        if !entries.is_empty() && !descriptor_heap_plan.backend_ready {
            return Err(format!(
                "scene texture descriptor heap requires a ready VK_EXT_descriptor_heap plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }

        Ok(Self {
            draw_binding_count: draw_bindings.len(),
            texture_count: entries.len(),
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            draw_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_unique_scene_texture_images",
                "allocate_or_reuse_descriptor_heap",
                "write_image_sampler_descriptors",
                "bind_descriptor_heap_offsets_per_draw",
            ],
            bindings,
        })
    }

    pub(in crate::renderer::native_vulkan) fn heap_index(
        &self,
        resource: SceneResourceId,
    ) -> Result<usize, String> {
        self.entries
            .iter()
            .find(|entry| entry.resource == resource)
            .map(|entry| entry.heap_index)
            .ok_or_else(|| format!("missing scene texture heap entry for {resource:?}"))
    }
}

fn heap_image_binding(
    descriptor: &NativeVulkanSceneTextureDescriptorBinding,
    binding: NativeVulkanSceneTextureImageBinding,
    heap_index: usize,
) -> Result<NativeVulkanSceneTextureHeapImageBinding, String> {
    validate_descriptor_binding_metadata(descriptor, binding)?;
    Ok(NativeVulkanSceneTextureHeapImageBinding {
        resource: binding.resource,
        heap_index,
        image: binding.image,
        format: binding.format,
        width: binding.width,
        height: binding.height,
        mip_count: binding.mip_count,
    })
}

fn heap_entry(
    descriptor: &NativeVulkanSceneTextureDescriptorBinding,
    binding: NativeVulkanSceneTextureImageBinding,
    heap_index: usize,
) -> NativeVulkanSceneTextureHeapEntry {
    NativeVulkanSceneTextureHeapEntry {
        resource: binding.resource,
        heap_index,
        image_handle: binding.image.as_raw(),
        view_handle: binding.view.as_raw(),
        sampler_handle: binding.sampler.as_raw(),
        format: format!("{:?}", binding.format),
        width: binding.width,
        height: binding.height,
        mip_count: binding.mip_count,
        shader_mapping: descriptor.shader_mapping,
    }
}

fn validate_descriptor_binding_metadata(
    descriptor: &NativeVulkanSceneTextureDescriptorBinding,
    binding: NativeVulkanSceneTextureImageBinding,
) -> Result<(), String> {
    if descriptor.role != SceneGraphResourceRole::BaseColor || descriptor.slot != 0 {
        return Err(format!(
            "scene texture heap only supports BaseColor slot 0, got {:?} slot {} for {:?}",
            descriptor.role, descriptor.slot, descriptor.resource
        ));
    }
    if descriptor.resource != binding.resource {
        return Err(format!(
            "scene texture heap binding returned {:?} for descriptor {:?}",
            binding.resource, descriptor.resource
        ));
    }
    validate_optional_descriptor_u32(
        descriptor.width,
        binding.width,
        descriptor.resource,
        "width",
    )?;
    validate_optional_descriptor_u32(
        descriptor.height,
        binding.height,
        descriptor.resource,
        "height",
    )?;
    validate_optional_descriptor_u32(
        descriptor.mip_count,
        binding.mip_count,
        descriptor.resource,
        "mip count",
    )?;
    let descriptor_format = descriptor.format.ok_or_else(|| {
        format!(
            "scene texture {:?} descriptor missing native format",
            descriptor.resource
        )
    })?;
    let expected_format = scene_texture_vk_format(descriptor_format);
    if expected_format != binding.format {
        return Err(format!(
            "scene texture {:?} descriptor format {:?} does not match retained image {:?}",
            descriptor.resource, expected_format, binding.format
        ));
    }
    Ok(())
}

fn validate_optional_descriptor_u32(
    descriptor_value: Option<u32>,
    binding_value: u32,
    resource: SceneResourceId,
    label: &'static str,
) -> Result<(), String> {
    match descriptor_value {
        Some(value) if value == binding_value => Ok(()),
        Some(value) => Err(format!(
            "scene texture {resource:?} descriptor {label} {value} does not match retained image {binding_value}"
        )),
        None => Err(format!(
            "scene texture {resource:?} descriptor missing {label}"
        )),
    }
}

fn scene_texture_vk_format(format: SceneTextureFormat) -> vk::Format {
    match format {
        SceneTextureFormat::Bc1RgbaUnormBlock => vk::Format::BC1_RGBA_UNORM_BLOCK,
        SceneTextureFormat::Bc3UnormBlock => vk::Format::BC3_UNORM_BLOCK,
        SceneTextureFormat::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
        SceneTextureFormat::R8Unorm => vk::Format::R8_UNORM,
        SceneTextureFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneObjectId;

    #[test]
    fn texture_heap_plan_deduplicates_resource_bindings_and_tracks_image_handle() {
        let descriptors = descriptor_frame_plan(vec![
            descriptor_binding(SceneObjectId(1), SceneResourceId(7)),
            descriptor_binding(SceneObjectId(2), SceneResourceId(7)),
            descriptor_binding(SceneObjectId(3), SceneResourceId(8)),
        ]);

        let plan = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            fake_binding,
        )
        .expect("texture heap plan");

        assert_eq!(plan.texture_count, 2);
        assert_eq!(plan.draw_binding_count, 3);
        assert_eq!(plan.entries[0].resource, SceneResourceId(7));
        assert_eq!(plan.entries[0].heap_index, 0);
        assert_eq!(plan.entries[0].image_handle, 107);
        assert_eq!(plan.entries[1].resource, SceneResourceId(8));
        assert_eq!(plan.entries[1].heap_index, 1);
        assert_eq!(plan.draw_bindings[1].heap_index, 0);
        assert!(plan.descriptor_heap_plan.backend_ready);
    }

    #[test]
    fn texture_heap_plan_rejects_missing_retained_image_binding() {
        let descriptors = descriptor_frame_plan(vec![descriptor_binding(
            SceneObjectId(1),
            SceneResourceId(7),
        )]);

        let err = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            |_| Err("missing retained image".to_owned()),
        )
        .expect_err("missing retained image must fail");

        assert!(err.contains("missing retained image"));
    }

    #[test]
    fn texture_heap_plan_rejects_descriptor_image_format_mismatch() {
        let descriptors = descriptor_frame_plan(vec![descriptor_binding(
            SceneObjectId(1),
            SceneResourceId(7),
        )]);

        let err = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            |resource| {
                Ok(NativeVulkanSceneTextureImageBinding {
                    format: vk::Format::BC7_UNORM_BLOCK,
                    ..fake_binding(resource)?
                })
            },
        )
        .expect_err("descriptor/image format mismatch must fail");

        assert!(err.contains("descriptor format"));
    }

    #[test]
    fn texture_heap_plan_rejects_unready_descriptor_heap_capabilities() {
        let descriptors = descriptor_frame_plan(vec![descriptor_binding(
            SceneObjectId(1),
            SceneResourceId(7),
        )]);

        let err = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            fake_binding,
        )
        .expect_err("descriptor heap must be ready for sampled textures");

        assert!(err.contains("requires a ready VK_EXT_descriptor_heap plan"));
    }

    #[test]
    fn texture_heap_plan_allows_texture_free_frame_without_heap_allocation() {
        let descriptors = NativeVulkanSceneTextureDescriptorFramePlan {
            draw_count: 1,
            binding_count: 0,
            bindings: Vec::new(),
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        };

        let plan = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            fake_binding,
        )
        .expect("texture-free frame does not need heap capabilities");

        assert_eq!(plan.texture_count, 0);
        assert_eq!(
            plan.descriptor_heap_plan.blocking_reason,
            Some("no-sampled-images")
        );
    }

    fn descriptor_frame_plan(
        bindings: Vec<NativeVulkanSceneTextureDescriptorBinding>,
    ) -> NativeVulkanSceneTextureDescriptorFramePlan {
        NativeVulkanSceneTextureDescriptorFramePlan {
            draw_count: bindings.len(),
            binding_count: bindings.len(),
            bindings,
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        }
    }

    fn descriptor_binding(
        object: SceneObjectId,
        resource: SceneResourceId,
    ) -> NativeVulkanSceneTextureDescriptorBinding {
        NativeVulkanSceneTextureDescriptorBinding {
            object,
            slot: 0,
            role: SceneGraphResourceRole::BaseColor,
            resource,
            width: Some(512),
            height: Some(256),
            format: Some(SceneTextureFormat::R8G8B8A8Unorm),
            mip_count: Some(4),
            payload_bytes: Some(524_288),
            shader_mapping: "set0.binding0.base_color",
        }
    }

    fn fake_binding(
        resource: SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String> {
        let raw = u64::from(100 + resource.0);
        Ok(NativeVulkanSceneTextureImageBinding {
            resource,
            image: vk::Image::from_raw(raw),
            view: vk::ImageView::from_raw(u64::from(200 + resource.0)),
            sampler: vk::Sampler::from_raw(u64::from(300 + resource.0)),
            format: vk::Format::R8G8B8A8_UNORM,
            width: 512,
            height: 256,
            mip_count: 4,
        })
    }

    fn ready_descriptor_heap_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            resource_heap_alignment: 64,
            sampler_heap_alignment: 64,
            max_resource_heap_size: 4096,
            min_resource_heap_reserved_range: 64,
            max_sampler_heap_size: 4096,
            min_sampler_heap_reserved_range: 64,
            image_descriptor_size: 32,
            sampler_descriptor_size: 16,
            image_descriptor_alignment: 32,
            sampler_descriptor_alignment: 16,
            ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
        }
    }
}
