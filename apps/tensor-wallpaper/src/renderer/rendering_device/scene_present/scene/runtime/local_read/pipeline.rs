//! Typed graphics-pipeline metadata for dynamic-rendering local read.

use std::collections::HashSet;

use vulkan_renderer::{
    Backend, RenderingLocalReadMapping, RenderingLocalReadMappingDescriptor,
    RenderingLocalReadMappingKind, TextureFormat,
};

use crate::renderer::rendering_device::scene::BuiltinSceneLocalReadShader;

use super::super::descriptor_layout::ScenePipelineShaderDescriptorAccess;

/// Fully typed graphics-pipeline metadata for one proven local-read scope.
///
/// Shader facts and logical attachment slots are validated here. Device limits
/// and Vulkan lowering remain owned by `vulkan-renderer` when `shared_mapping`
/// creates the executable mapping for the selected device.
#[derive(Debug, Clone)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) struct SceneLocalReadPipelineMetadata<
    'a,
> {
    shader: Option<&'a BuiltinSceneLocalReadShader>,
    color_attachment_formats: Vec<TextureFormat>,
    color_attachment_locations: Vec<Option<u32>>,
    color_attachment_input_indices: Vec<Option<u32>>,
}

impl<'a> SceneLocalReadPipelineMetadata<'a> {
    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn new(
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        shader: Option<&'a BuiltinSceneLocalReadShader>,
        color_attachment_formats: &[TextureFormat],
        color_attachment_locations: &[Option<u32>],
        color_attachment_input_indices: &[Option<u32>],
    ) -> Result<Self, String> {
        let shader = validate_scene_local_read_shader_variant(descriptor_access, shader)?;
        let shader_indices = shader
            .input_attachments
            .iter()
            .map(|input| input.input_attachment_index)
            .collect::<Vec<_>>();
        validate_unique_values(&shader_indices, "shader input attachment index")?;
        validate_unique_values(
            shader.color_output_locations,
            "shader color output location",
        )?;
        validate_mapping_shape(
            color_attachment_formats,
            color_attachment_locations,
            color_attachment_input_indices,
            "local-read pipeline",
        )?;
        validate_unique_optional_values(color_attachment_locations, "color attachment location")?;
        validate_unique_optional_values(color_attachment_input_indices, "input attachment index")?;
        if !color_attachment_input_indices.iter().any(Option::is_some) {
            return Err("local-read mapping has no input attachment index".to_owned());
        }
        validate_exact_mapped_values(
            color_attachment_locations,
            shader.color_output_locations,
            "color attachment location",
        )?;
        validate_exact_mapped_values(
            color_attachment_input_indices,
            &shader_indices,
            "input attachment index",
        )?;

        Ok(Self {
            shader: Some(shader),
            color_attachment_formats: color_attachment_formats.to_vec(),
            color_attachment_locations: color_attachment_locations.to_vec(),
            color_attachment_input_indices: color_attachment_input_indices.to_vec(),
        })
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn output_only(
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        color_attachment_formats: &[TextureFormat],
        color_attachment_locations: &[Option<u32>],
    ) -> Result<Self, String> {
        validate_unique_values(&descriptor_access.sampled_slots, "sampled slot")?;
        if !descriptor_access.input_attachment_slots.is_empty() {
            return Err(
                "local-read producer pipeline cannot declare input-attachment slots".to_owned(),
            );
        }
        let input_indices = vec![None; color_attachment_locations.len()];
        validate_mapping_shape(
            color_attachment_formats,
            color_attachment_locations,
            &input_indices,
            "local-read producer",
        )?;
        validate_exact_mapped_values(
            color_attachment_locations,
            &[0],
            "color attachment location",
        )?;
        validate_unique_optional_values(color_attachment_locations, "color attachment location")?;

        Ok(Self {
            shader: None,
            color_attachment_formats: color_attachment_formats.to_vec(),
            color_attachment_locations: color_attachment_locations.to_vec(),
            color_attachment_input_indices: input_indices,
        })
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn local_read_fragment_spirv(
        &self,
    ) -> Option<&'a [u32]> {
        self.shader.map(|shader| shader.fragment_spirv)
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn color_attachment_formats(
        &self,
    ) -> &[TextureFormat] {
        &self.color_attachment_formats
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn active_color_attachments(
        &self,
    ) -> Vec<bool> {
        self.color_attachment_locations
            .iter()
            .map(Option::is_some)
            .collect()
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn shared_mapping(
        &self,
        device: &Backend,
    ) -> Result<RenderingLocalReadMapping, String> {
        device
            .create_rendering_local_read_mapping(RenderingLocalReadMappingDescriptor {
                color_attachment_locations: &self.color_attachment_locations,
                color_attachment_input_indices: &self.color_attachment_input_indices,
                kind: if self.shader.is_some() {
                    RenderingLocalReadMappingKind::InputAttachment
                } else {
                    RenderingLocalReadMappingKind::OutputOnly
                },
            })
            .map_err(|error| format!("create scene local-read mapping: {error}"))
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn input_attachment_binding(
        &self,
        slot: u32,
    ) -> Option<u32> {
        self.shader?
            .input_attachments
            .iter()
            .find(|input| input.slot == slot)
            .map(|input| input.binding)
    }
}

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn validate_scene_local_read_shader_variant<
    'a,
>(
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    shader: Option<&'a BuiltinSceneLocalReadShader>,
) -> Result<&'a BuiltinSceneLocalReadShader, String> {
    validate_unique_values(&descriptor_access.sampled_slots, "sampled slot")?;
    validate_unique_values(
        &descriptor_access.input_attachment_slots,
        "input-attachment slot",
    )?;
    if descriptor_access.input_attachment_slots.is_empty() {
        return Err(
            "local-read pipeline metadata requires a typed input-attachment slot".to_owned(),
        );
    }
    if descriptor_access
        .sampled_slots
        .iter()
        .any(|slot| descriptor_access.input_attachment_slots.contains(slot))
    {
        return Err(
            "local-read pipeline descriptor access overlaps sampled and input-attachment slots"
                .to_owned(),
        );
    }
    let shader = shader.ok_or_else(|| {
        "local-read pipeline shader has no explicit subpassInput variant".to_owned()
    })?;
    if shader.fragment_spirv.is_empty() {
        return Err("local-read pipeline subpassInput shader variant is empty".to_owned());
    }
    if shader.input_attachments.is_empty() {
        return Err("local-read pipeline shader interface has no input attachments".to_owned());
    }
    if shader.color_output_locations.is_empty() {
        return Err("local-read pipeline shader interface has no color outputs".to_owned());
    }

    let shader_slots = shader
        .input_attachments
        .iter()
        .map(|input| input.slot)
        .collect::<Vec<_>>();
    let shader_indices = shader
        .input_attachments
        .iter()
        .map(|input| input.input_attachment_index)
        .collect::<Vec<_>>();
    let shader_bindings = shader
        .input_attachments
        .iter()
        .map(|input| input.binding)
        .collect::<Vec<_>>();
    validate_unique_values(&shader_slots, "shader input-attachment slot")?;
    validate_unique_values(&shader_indices, "shader input attachment index")?;
    validate_unique_values(&shader_bindings, "shader input-attachment binding")?;
    validate_unique_values(
        shader.color_output_locations,
        "shader color output location",
    )?;
    if !same_value_set(&descriptor_access.input_attachment_slots, &shader_slots) {
        return Err(format!(
            "local-read descriptor input slots {:?} do not match shader interface {:?}",
            descriptor_access.input_attachment_slots, shader_slots
        ));
    }
    Ok(shader)
}

fn validate_mapping_shape(
    formats: &[TextureFormat],
    locations: &[Option<u32>],
    input_indices: &[Option<u32>],
    label: &str,
) -> Result<(), String> {
    if formats.len() != locations.len() || formats.len() != input_indices.len() {
        return Err(format!(
            "{label} format/location/input arrays differ in length ({} vs {} vs {})",
            formats.len(),
            locations.len(),
            input_indices.len()
        ));
    }
    if formats.is_empty() {
        return Err(format!("{label} has no color attachments"));
    }
    Ok(())
}

fn validate_exact_mapped_values(
    mapped: &[Option<u32>],
    required: &[u32],
    label: &str,
) -> Result<(), String> {
    let mapped = mapped.iter().copied().flatten().collect::<Vec<_>>();
    if !same_value_set(&mapped, required) {
        return Err(format!(
            "local-read mapped {label}s {mapped:?} do not match shader interface {required:?}"
        ));
    }
    Ok(())
}

fn validate_unique_optional_values(values: &[Option<u32>], label: &str) -> Result<(), String> {
    validate_unique_values(&values.iter().copied().flatten().collect::<Vec<_>>(), label)
}

fn same_value_set(left: &[u32], right: &[u32]) -> bool {
    left.len() == right.len()
        && left.iter().all(|value| right.contains(value))
        && right.iter().all(|value| left.contains(value))
}

fn validate_unique_values(values: &[u32], label: &str) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        if !seen.insert(*value) {
            return Err(format!("local-read {label} {value} is duplicated"));
        }
    }
    Ok(())
}
