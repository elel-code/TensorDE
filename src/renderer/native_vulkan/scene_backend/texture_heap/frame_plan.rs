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
use super::texture_set::{NativeVulkanSceneTextureSetBinding, NativeVulkanSceneTextureSetKey};

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
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub heap_index: usize,
    pub image_handle: u64,
    pub view_handle: u64,
    pub sampler_handle: u64,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapDrawBinding {
    pub object: SceneObjectId,
    pub draw_index: usize,
    pub texture_set: NativeVulkanSceneTextureSetKey,
    pub base_heap_index: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
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
        let descriptors_by_draw = descriptors_by_draw(descriptors)?;
        let mut texture_set_to_slice =
            BTreeMap::<NativeVulkanSceneTextureSetKey, (usize, usize)>::new();
        let mut entries = Vec::new();
        let mut bindings = Vec::new();
        let mut draw_bindings = Vec::with_capacity(descriptors.draw_count);

        for (draw_index, draw_descriptors) in descriptors_by_draw.iter().enumerate() {
            if draw_descriptors.is_empty() {
                continue;
            }
            let texture_set = texture_set_key_from_descriptors(draw_descriptors)?;
            let (base_heap_index, texture_count) =
                if let Some(slice) = texture_set_to_slice.get(&texture_set).copied() {
                    slice
                } else {
                    let base_heap_index = entries.len();
                    for (ordinal, descriptor) in draw_descriptors.iter().enumerate() {
                        let heap_index = base_heap_index + ordinal;
                        let binding = resolve_binding(descriptor.resource)?;
                        let image_binding = heap_image_binding(descriptor, binding, heap_index)?;
                        let entry = heap_entry(descriptor, binding, heap_index);
                        entries.push(entry);
                        bindings.push(image_binding);
                    }
                    let slice = (base_heap_index, draw_descriptors.len());
                    texture_set_to_slice.insert(texture_set.clone(), slice);
                    slice
                };
            let object = draw_descriptors[0].object;
            draw_bindings.push(NativeVulkanSceneTextureHeapDrawBinding {
                object,
                draw_index,
                shader_mappings: texture_set.shader_mappings(),
                texture_set,
                base_heap_index,
                texture_count,
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

    pub(in crate::renderer::native_vulkan) fn texture_set_slice(
        &self,
        texture_set: &NativeVulkanSceneTextureSetKey,
    ) -> Result<(usize, usize), String> {
        self.draw_bindings
            .iter()
            .find(|binding| &binding.texture_set == texture_set)
            .map(|binding| (binding.base_heap_index, binding.texture_count))
            .ok_or_else(|| format!("missing scene texture heap slice for {texture_set:?}"))
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
        slot: descriptor.slot,
        role: descriptor.role,
        heap_index,
        image_handle: binding.image.as_raw(),
        view_handle: binding.view.as_raw(),
        sampler_handle: binding.sampler.as_raw(),
        format: format!("{:?}", binding.format),
        width: binding.width,
        height: binding.height,
        mip_count: binding.mip_count,
        shader_mapping: descriptor.shader_mapping.clone(),
    }
}

fn validate_descriptor_binding_metadata(
    descriptor: &NativeVulkanSceneTextureDescriptorBinding,
    binding: NativeVulkanSceneTextureImageBinding,
) -> Result<(), String> {
    let texture_index = descriptor.role.shader_texture_index();
    if texture_index != descriptor.slot {
        return Err(format!(
            "scene texture heap descriptor slot {} does not match WE g_Texture{} role for {:?}",
            descriptor.slot, texture_index, descriptor.resource
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

fn descriptors_by_draw(
    descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
) -> Result<Vec<Vec<&NativeVulkanSceneTextureDescriptorBinding>>, String> {
    let mut by_draw = vec![Vec::new(); descriptors.draw_count];
    for descriptor in &descriptors.bindings {
        let draw = by_draw.get_mut(descriptor.draw_index).ok_or_else(|| {
            format!(
                "scene texture descriptor draw index {} exceeds draw count {}",
                descriptor.draw_index, descriptors.draw_count
            )
        })?;
        draw.push(descriptor);
    }
    for draw in &mut by_draw {
        draw.sort_by_key(|descriptor| descriptor.slot);
    }
    Ok(by_draw)
}

fn texture_set_key_from_descriptors(
    descriptors: &[&NativeVulkanSceneTextureDescriptorBinding],
) -> Result<NativeVulkanSceneTextureSetKey, String> {
    let mut bindings = Vec::with_capacity(descriptors.len());
    let mut seen_slots = std::collections::BTreeSet::new();
    for descriptor in descriptors {
        if !seen_slots.insert(descriptor.slot) {
            return Err(format!(
                "duplicate scene texture descriptor slot {} for object {:?}",
                descriptor.slot, descriptor.object
            ));
        }
        bindings.push(NativeVulkanSceneTextureSetBinding {
            slot: descriptor.slot,
            role: descriptor.role,
            resource: descriptor.resource,
        });
    }
    Ok(NativeVulkanSceneTextureSetKey { bindings })
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
    use super::super::texture_set::scene_shader_texture_mapping;
    use super::*;
    use crate::engine::scene_engine::SceneObjectId;

    #[test]
    fn texture_heap_plan_deduplicates_resource_bindings_and_tracks_image_handle() {
        let descriptors = descriptor_frame_plan(vec![
            vec![descriptor_binding(
                0,
                SceneObjectId(1),
                0,
                SceneResourceId(7),
            )],
            vec![descriptor_binding(
                1,
                SceneObjectId(2),
                0,
                SceneResourceId(7),
            )],
            vec![descriptor_binding(
                2,
                SceneObjectId(3),
                0,
                SceneResourceId(8),
            )],
        ]);

        let plan = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            retained_image_binding,
        )
        .expect("texture heap plan");

        assert_eq!(plan.texture_count, 2);
        assert_eq!(plan.draw_binding_count, 3);
        assert_eq!(plan.entries[0].resource, SceneResourceId(7));
        assert_eq!(plan.entries[0].heap_index, 0);
        assert_eq!(plan.entries[0].image_handle, 107);
        assert_eq!(plan.entries[1].resource, SceneResourceId(8));
        assert_eq!(plan.entries[1].heap_index, 1);
        assert_eq!(plan.draw_bindings[1].base_heap_index, 0);
        assert!(plan.descriptor_heap_plan.backend_ready);
    }

    #[test]
    fn texture_heap_plan_allocates_contiguous_slice_per_we_texture_set() {
        let descriptors = descriptor_frame_plan(vec![
            vec![
                descriptor_binding(0, SceneObjectId(1), 0, SceneResourceId(7)),
                descriptor_binding(0, SceneObjectId(1), 4, SceneResourceId(8)),
            ],
            vec![
                descriptor_binding(1, SceneObjectId(2), 0, SceneResourceId(7)),
                descriptor_binding(1, SceneObjectId(2), 4, SceneResourceId(8)),
            ],
        ]);

        let plan = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            retained_image_binding,
        )
        .expect("texture heap plan");

        assert_eq!(plan.texture_count, 2);
        assert_eq!(plan.draw_binding_count, 2);
        assert_eq!(plan.entries[0].slot, 0);
        assert_eq!(plan.entries[1].slot, 4);
        assert_eq!(plan.draw_bindings[0].base_heap_index, 0);
        assert_eq!(plan.draw_bindings[0].texture_count, 2);
        assert_eq!(plan.draw_bindings[1].base_heap_index, 0);
        assert_eq!(
            plan.draw_bindings[0].shader_mappings,
            vec![
                "set0.binding0.g_Texture0".to_owned(),
                "set0.binding4.g_Texture4".to_owned()
            ]
        );
    }

    #[test]
    fn texture_heap_plan_rejects_missing_retained_image_binding() {
        let descriptors = descriptor_frame_plan(vec![vec![descriptor_binding(
            0,
            SceneObjectId(1),
            0,
            SceneResourceId(7),
        )]]);

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
        let descriptors = descriptor_frame_plan(vec![vec![descriptor_binding(
            0,
            SceneObjectId(1),
            0,
            SceneResourceId(7),
        )]]);

        let err = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            ready_descriptor_heap_properties(),
            |resource| {
                Ok(NativeVulkanSceneTextureImageBinding {
                    format: vk::Format::BC7_UNORM_BLOCK,
                    ..retained_image_binding(resource)?
                })
            },
        )
        .expect_err("descriptor/image format mismatch must fail");

        assert!(err.contains("descriptor format"));
    }

    #[test]
    fn texture_heap_plan_rejects_unready_descriptor_heap_capabilities() {
        let descriptors = descriptor_frame_plan(vec![vec![descriptor_binding(
            0,
            SceneObjectId(1),
            0,
            SceneResourceId(7),
        )]]);

        let err = NativeVulkanSceneTextureHeapFramePlan::from_texture_descriptors(
            &descriptors,
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            retained_image_binding,
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
            retained_image_binding,
        )
        .expect("texture-free frame does not need heap capabilities");

        assert_eq!(plan.texture_count, 0);
        assert_eq!(
            plan.descriptor_heap_plan.blocking_reason,
            Some("no-sampled-images")
        );
    }

    fn descriptor_frame_plan(
        draw_bindings: Vec<Vec<NativeVulkanSceneTextureDescriptorBinding>>,
    ) -> NativeVulkanSceneTextureDescriptorFramePlan {
        let bindings = draw_bindings.into_iter().flatten().collect::<Vec<_>>();
        NativeVulkanSceneTextureDescriptorFramePlan {
            draw_count: bindings
                .iter()
                .map(|binding| binding.draw_index + 1)
                .max()
                .unwrap_or_default(),
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
        draw_index: usize,
        object: SceneObjectId,
        slot: u32,
        resource: SceneResourceId,
    ) -> NativeVulkanSceneTextureDescriptorBinding {
        NativeVulkanSceneTextureDescriptorBinding {
            draw_index,
            object,
            slot,
            role: SceneGraphResourceRole::shader_texture(slot),
            resource,
            width: Some(512),
            height: Some(256),
            format: Some(SceneTextureFormat::R8G8B8A8Unorm),
            mip_count: Some(4),
            payload_bytes: Some(524_288),
            shader_mapping: scene_shader_texture_mapping(slot),
        }
    }

    fn retained_image_binding(
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
