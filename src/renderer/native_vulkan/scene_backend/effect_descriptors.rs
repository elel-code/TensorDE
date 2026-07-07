//! Effect pass sampled-texture descriptor planning.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::BTreeSet;

use serde::Serialize;

use crate::engine::scene_engine::{
    SCENE_WE_MAX_SHADER_TEXTURE_SLOTS, SceneEffectPassGraphInputSource,
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphPlan, SceneGraphResourceRole,
    SceneGraphTarget, SceneObjectId, SceneResourceId, SceneTextureResidency,
};

use super::resource_heap::texture_set::scene_shader_texture_mapping;
use super::texture_descriptors::{
    NativeVulkanSceneTargetInputTextureDescriptor, NativeVulkanSceneTextureDescriptorFormat,
    NativeVulkanSceneTextureDescriptorSource, NativeVulkanSceneTextureDescriptorVkFormat,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTextureDescriptorFramePlan {
    pub pass_count: usize,
    pub binding_count: usize,
    pub bindings: Vec<NativeVulkanSceneEffectTextureDescriptorBinding>,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTextureDescriptorBinding {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub source: NativeVulkanSceneTextureDescriptorSource,
    pub width: u32,
    pub height: u32,
    pub format: NativeVulkanSceneTextureDescriptorFormat,
    pub mip_count: u32,
    pub payload_bytes: Option<u64>,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectExternalTextureDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: NativeVulkanSceneTextureDescriptorVkFormat,
}

impl NativeVulkanSceneEffectTextureDescriptorFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph<
        TextureResidency,
        TargetInput,
        PreviousInput,
        SceneInput,
    >(
        graph: &SceneEffectPassGraphPlan,
        mut texture_residency: TextureResidency,
        mut target_input: TargetInput,
        mut previous_input: PreviousInput,
        mut scene_input: SceneInput,
    ) -> Result<Self, String>
    where
        TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
        TargetInput: FnMut(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneTargetInputTextureDescriptor, String>,
        PreviousInput: FnMut(
            SceneObjectId,
            usize,
        )
            -> Result<NativeVulkanSceneEffectExternalTextureDescriptor, String>,
        SceneInput: FnMut(
            SceneObjectId,
            usize,
        )
            -> Result<NativeVulkanSceneEffectExternalTextureDescriptor, String>,
    {
        let mut bindings = Vec::new();
        for pass in &graph.passes {
            let mut used_slots = BTreeSet::new();
            if let Some(source) = pass.source.as_ref() {
                push_effect_input_binding(
                    &mut bindings,
                    &mut used_slots,
                    pass,
                    source.slot,
                    &source.source,
                    &mut texture_residency,
                    &mut target_input,
                    &mut previous_input,
                    &mut scene_input,
                )?;
            }
            for input in &pass.input_bindings {
                push_effect_input_binding(
                    &mut bindings,
                    &mut used_slots,
                    pass,
                    input.slot,
                    &input.source,
                    &mut texture_residency,
                    &mut target_input,
                    &mut previous_input,
                    &mut scene_input,
                )?;
            }
            for resource in &pass.texture_resources {
                push_effect_resident_texture_binding(
                    &mut bindings,
                    &mut used_slots,
                    pass,
                    resource.slot,
                    resource.resource,
                    &mut texture_residency,
                )?;
            }
        }
        Ok(Self {
            pass_count: graph.passes.len(),
            binding_count: bindings.len(),
            bindings,
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_effect_source_texture_descriptors",
                "resolve_effect_named_fbo_texture_descriptors",
                "resolve_effect_previous_scene_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        })
    }
}

fn push_effect_input_binding<TextureResidency, TargetInput, PreviousInput, SceneInput>(
    bindings: &mut Vec<NativeVulkanSceneEffectTextureDescriptorBinding>,
    used_slots: &mut BTreeSet<u32>,
    pass: &SceneEffectPassGraphMaterialPass,
    slot: u32,
    source: &SceneEffectPassGraphInputSource,
    texture_residency: &mut TextureResidency,
    target_input: &mut TargetInput,
    previous_input: &mut PreviousInput,
    scene_input: &mut SceneInput,
) -> Result<(), String>
where
    TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
    TargetInput:
        FnMut(SceneGraphTarget) -> Result<NativeVulkanSceneTargetInputTextureDescriptor, String>,
    PreviousInput: FnMut(
        SceneObjectId,
        usize,
    ) -> Result<NativeVulkanSceneEffectExternalTextureDescriptor, String>,
    SceneInput: FnMut(
        SceneObjectId,
        usize,
    ) -> Result<NativeVulkanSceneEffectExternalTextureDescriptor, String>,
{
    validate_effect_texture_slot(pass, used_slots, slot)?;
    let descriptor = match source {
        SceneEffectPassGraphInputSource::ObjectSourceTexture(resource) => {
            effect_resident_texture_descriptor(pass, slot, *resource, texture_residency)?
        }
        SceneEffectPassGraphInputSource::GraphTarget(target) => {
            let input = target_input(*target)?;
            if input.target != *target {
                return Err(format!(
                    "scene effect texture descriptor target resolver returned {:?} for requested {:?}",
                    input.target, target
                ));
            }
            effect_vk_texture_descriptor(
                pass,
                slot,
                NativeVulkanSceneTextureDescriptorSource::GraphTarget(*target),
                input.width,
                input.height,
                input.format,
            )?
        }
        SceneEffectPassGraphInputSource::PreviousFramebuffer => {
            let input = previous_input(pass.object, pass.graph_pass_index)?;
            effect_vk_texture_descriptor(
                pass,
                slot,
                NativeVulkanSceneTextureDescriptorSource::PreviousFramebuffer {
                    object: pass.object,
                    effect_pass_index: pass.graph_pass_index,
                },
                input.width,
                input.height,
                input.format,
            )?
        }
        SceneEffectPassGraphInputSource::Scene => {
            let input = scene_input(pass.object, pass.graph_pass_index)?;
            effect_vk_texture_descriptor(
                pass,
                slot,
                NativeVulkanSceneTextureDescriptorSource::Scene {
                    object: pass.object,
                    effect_pass_index: pass.graph_pass_index,
                },
                input.width,
                input.height,
                input.format,
            )?
        }
    };
    bindings.push(descriptor);
    Ok(())
}

fn push_effect_resident_texture_binding<TextureResidency>(
    bindings: &mut Vec<NativeVulkanSceneEffectTextureDescriptorBinding>,
    used_slots: &mut BTreeSet<u32>,
    pass: &SceneEffectPassGraphMaterialPass,
    slot: u32,
    resource: SceneResourceId,
    texture_residency: &mut TextureResidency,
) -> Result<(), String>
where
    TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
{
    validate_effect_texture_slot(pass, used_slots, slot)?;
    bindings.push(effect_resident_texture_descriptor(
        pass,
        slot,
        resource,
        texture_residency,
    )?);
    Ok(())
}

fn validate_effect_texture_slot(
    pass: &SceneEffectPassGraphMaterialPass,
    used_slots: &mut BTreeSet<u32>,
    slot: u32,
) -> Result<(), String> {
    if slot >= SCENE_WE_MAX_SHADER_TEXTURE_SLOTS {
        return Err(format!(
            "scene effect pass {} for object {:?} texture slot {} exceeds WE slot mask width {}",
            pass.pass_index, pass.object, slot, SCENE_WE_MAX_SHADER_TEXTURE_SLOTS
        ));
    }
    if !used_slots.insert(slot) {
        return Err(format!(
            "scene effect pass {} for object {:?} binds texture slot {} more than once",
            pass.pass_index, pass.object, slot
        ));
    }
    Ok(())
}

fn effect_resident_texture_descriptor<TextureResidency>(
    pass: &SceneEffectPassGraphMaterialPass,
    slot: u32,
    resource: SceneResourceId,
    texture_residency: &mut TextureResidency,
) -> Result<NativeVulkanSceneEffectTextureDescriptorBinding, String>
where
    TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
{
    let texture = texture_residency(resource).ok_or_else(|| {
        format!(
            "missing resident scene texture {:?} for effect pass {} object {:?}",
            resource, pass.pass_index, pass.object
        )
    })?;
    if texture.id != resource {
        return Err(format!(
            "scene effect texture resolver returned {:?} for requested {:?}",
            texture.id, resource
        ));
    }
    let width = texture.width.ok_or_else(|| {
        format!(
            "scene effect texture descriptor {:?} missing width",
            resource
        )
    })?;
    let height = texture.height.ok_or_else(|| {
        format!(
            "scene effect texture descriptor {:?} missing height",
            resource
        )
    })?;
    let format = texture.format.ok_or_else(|| {
        format!(
            "scene effect texture descriptor {:?} missing native format",
            resource
        )
    })?;
    let mip_count = texture.mip_count.ok_or_else(|| {
        format!(
            "scene effect texture descriptor {:?} missing mip count",
            resource
        )
    })?;
    let payload_bytes = texture.payload_bytes.ok_or_else(|| {
        format!(
            "scene effect texture descriptor {:?} missing payload byte count",
            resource
        )
    })?;
    validate_effect_texture_extent(pass, width, height, mip_count)?;
    Ok(NativeVulkanSceneEffectTextureDescriptorBinding {
        effect_pass_index: pass.graph_pass_index,
        object: pass.object,
        slot,
        role: SceneGraphResourceRole::shader_texture(slot),
        source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(resource),
        width,
        height,
        format: NativeVulkanSceneTextureDescriptorFormat::SceneTexture(format),
        mip_count,
        payload_bytes: Some(payload_bytes),
        shader_mapping: scene_shader_texture_mapping(slot),
    })
}

fn effect_vk_texture_descriptor(
    pass: &SceneEffectPassGraphMaterialPass,
    slot: u32,
    source: NativeVulkanSceneTextureDescriptorSource,
    width: u32,
    height: u32,
    format: NativeVulkanSceneTextureDescriptorVkFormat,
) -> Result<NativeVulkanSceneEffectTextureDescriptorBinding, String> {
    validate_effect_texture_extent(pass, width, height, 1)?;
    Ok(NativeVulkanSceneEffectTextureDescriptorBinding {
        effect_pass_index: pass.graph_pass_index,
        object: pass.object,
        slot,
        role: SceneGraphResourceRole::shader_texture(slot),
        source,
        width,
        height,
        format: NativeVulkanSceneTextureDescriptorFormat::VkFormat(format),
        mip_count: 1,
        payload_bytes: None,
        shader_mapping: scene_shader_texture_mapping(slot),
    })
}

fn validate_effect_texture_extent(
    pass: &SceneEffectPassGraphMaterialPass,
    width: u32,
    height: u32,
    mip_count: u32,
) -> Result<(), String> {
    if width == 0 || height == 0 || mip_count == 0 {
        return Err(format!(
            "scene effect pass {} for object {:?} has invalid texture metadata {width}x{height} mips={mip_count}",
            pass.pass_index, pass.object
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectPassGraphOutput, SceneEffectTextureResourceBinding, SceneResourceId,
        SceneTextureFormat, we::WeEffectKind,
    };

    #[test]
    fn effect_texture_descriptor_plan_resolves_source_fbo_and_resource_slots() {
        let graph = graph(vec![pass(
            0,
            Some(input(
                0,
                SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(9)),
            )),
            vec![input(
                1,
                SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(3)),
            )],
            vec![SceneEffectTextureResourceBinding {
                slot: 2,
                resource: SceneResourceId(12),
            }],
        )]);

        let plan = NativeVulkanSceneEffectTextureDescriptorFramePlan::from_effect_pass_graph(
            &graph,
            resident_texture,
            |target| {
                Ok(NativeVulkanSceneTargetInputTextureDescriptor {
                    target,
                    width: 1920,
                    height: 1080,
                    format: NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat,
                })
            },
            |_object, _pass| unreachable!("no previous framebuffer input"),
            |_object, _pass| unreachable!("no scene input"),
        )
        .expect("effect texture descriptor plan");

        assert_eq!(plan.pass_count, 1);
        assert_eq!(plan.binding_count, 3);
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(
            plan.bindings[0].source,
            NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(9))
        );
        assert_eq!(plan.bindings[0].shader_mapping, "set0.binding0.g_Texture0");
        assert_eq!(
            plan.bindings[1].source,
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(SceneGraphTarget::NamedFbo(3))
        );
        assert_eq!(
            plan.bindings[1].format,
            NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat
            )
        );
        assert_eq!(
            plan.bindings[2].source,
            NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(12))
        );
    }

    #[test]
    fn effect_texture_descriptor_plan_resolves_previous_and_scene_domains() {
        let graph = graph(vec![pass(
            4,
            Some(input(
                0,
                SceneEffectPassGraphInputSource::PreviousFramebuffer,
            )),
            vec![input(1, SceneEffectPassGraphInputSource::Scene)],
            Vec::new(),
        )]);

        let plan = NativeVulkanSceneEffectTextureDescriptorFramePlan::from_effect_pass_graph(
            &graph,
            |_| None,
            |_target| unreachable!("no graph target input"),
            |_object, _pass| {
                Ok(NativeVulkanSceneEffectExternalTextureDescriptor {
                    width: 3840,
                    height: 2160,
                    format: NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
                })
            },
            |_object, _pass| {
                Ok(NativeVulkanSceneEffectExternalTextureDescriptor {
                    width: 3840,
                    height: 2160,
                    format: NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
                })
            },
        )
        .expect("effect texture descriptor plan");

        assert_eq!(
            plan.bindings[0].source,
            NativeVulkanSceneTextureDescriptorSource::PreviousFramebuffer {
                object: SceneObjectId(7),
                effect_pass_index: 4,
            }
        );
        assert_eq!(
            plan.bindings[1].source,
            NativeVulkanSceneTextureDescriptorSource::Scene {
                object: SceneObjectId(7),
                effect_pass_index: 4,
            }
        );
    }

    #[test]
    fn effect_texture_descriptor_plan_rejects_duplicate_slots() {
        let graph = graph(vec![pass(
            0,
            Some(input(
                0,
                SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(9)),
            )),
            Vec::new(),
            vec![SceneEffectTextureResourceBinding {
                slot: 0,
                resource: SceneResourceId(12),
            }],
        )]);

        let err = NativeVulkanSceneEffectTextureDescriptorFramePlan::from_effect_pass_graph(
            &graph,
            resident_texture,
            |_target| unreachable!("no graph target input"),
            |_object, _pass| unreachable!("no previous framebuffer input"),
            |_object, _pass| unreachable!("no scene input"),
        )
        .expect_err("duplicate slot must fail");

        assert!(err.contains("binds texture slot 0 more than once"));
    }

    fn resident_texture(resource: SceneResourceId) -> Option<SceneTextureResidency> {
        matches!(resource, SceneResourceId(9) | SceneResourceId(12)).then_some(
            SceneTextureResidency {
                id: resource,
                width: Some(1024),
                height: Some(512),
                format: Some(SceneTextureFormat::R8G8B8A8Unorm),
                mip_count: Some(10),
                payload_bytes: Some(2_796_204),
            },
        )
    }

    fn graph(passes: Vec<SceneEffectPassGraphMaterialPass>) -> SceneEffectPassGraphPlan {
        SceneEffectPassGraphPlan {
            material_pass_count: passes.len(),
            passes,
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn input(
        slot: u32,
        source: SceneEffectPassGraphInputSource,
    ) -> SceneEffectPassGraphInputBinding {
        SceneEffectPassGraphInputBinding {
            slot,
            image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
            source,
        }
    }

    fn pass(
        graph_pass_index: usize,
        source: Option<SceneEffectPassGraphInputBinding>,
        input_bindings: Vec<SceneEffectPassGraphInputBinding>,
        texture_resources: Vec<SceneEffectTextureResourceBinding>,
    ) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: graph_pass_index,
            graph_pass_index,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: graph_pass_index,
            effect_file: "effects/test/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some("effects/test".to_owned()),
            source,
            input_bindings,
            output: SceneEffectPassGraphOutput::ObjectFinal(SceneObjectId(7)),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources,
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }
}
