//! Scene effect descriptor heap slice bind command recording.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{
    SCENE_GPU_IRIS_EFFECT_FRAGMENT_UNIFORM_BYTES, SCENE_GPU_IRIS_EFFECT_VERTEX_UNIFORM_BYTES,
    SceneEffectPassGraphMaterialPass, SceneObjectId,
};

use super::super::effect_uniforms::NativeVulkanSceneEffectUniformStage;
use super::key::{NativeVulkanSceneEffectTextureSetKey, effect_pass_texture_set_key};
use super::store::NativeVulkanSceneEffectResourceHeapPassBindInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneEffectResourceHeapPassBindPlan {
    pub(in crate::renderer::native_vulkan) effect_pass_index: usize,
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) heap_slice_index: usize,
    pub(in crate::renderer::native_vulkan) effect_uniform_buffer_count: usize,
    pub(in crate::renderer::native_vulkan) texture_set: NativeVulkanSceneEffectTextureSetKey,
    pub(in crate::renderer::native_vulkan) base_resource_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) resource_descriptor_count: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneEffectResourceHeapPassBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_pass_and_bind_info(
        pass: &SceneEffectPassGraphMaterialPass,
        bind_info: &NativeVulkanSceneEffectResourceHeapPassBindInfo,
    ) -> Result<Self, String> {
        let texture_set = effect_pass_texture_set_key(pass)?;
        if pass.graph_pass_index != bind_info.effect_pass_index {
            return Err(format!(
                "scene effect resource heap bind pass-index mismatch for object {:?}: pass {}, heap {}",
                pass.object, pass.graph_pass_index, bind_info.effect_pass_index
            ));
        }
        if pass.object != bind_info.object {
            return Err(format!(
                "scene effect resource heap bind object mismatch: pass {:?}, heap {:?}",
                pass.object, bind_info.object
            ));
        }
        if texture_set != bind_info.texture_set {
            return Err(format!(
                "scene effect resource heap bind texture-set mismatch for pass {} object {:?}: pass {:?}, heap {:?}",
                pass.graph_pass_index, pass.object, texture_set, bind_info.texture_set
            ));
        }
        if texture_set.bindings.len() != bind_info.texture_count {
            return Err(format!(
                "scene effect resource heap bind texture count mismatch for pass {} object {:?}: pass {}, heap {}",
                pass.graph_pass_index,
                pass.object,
                texture_set.bindings.len(),
                bind_info.texture_count
            ));
        }
        validate_effect_uniform_metadata_counts(pass, bind_info)?;
        let expected_resource_descriptor_count = texture_set
            .bindings
            .len()
            .saturating_add(bind_info.effect_uniform_buffer_count);
        if expected_resource_descriptor_count != bind_info.resource_descriptor_count {
            return Err(format!(
                "scene effect resource heap bind resource descriptor count mismatch for pass {} object {:?}: expected {}, heap {}",
                pass.graph_pass_index,
                pass.object,
                expected_resource_descriptor_count,
                bind_info.resource_descriptor_count
            ));
        }
        validate_iris_effect_uniform_contract(pass, bind_info)?;
        Ok(Self {
            effect_pass_index: pass.graph_pass_index,
            object: pass.object,
            heap_slice_index: bind_info.heap_slice_index,
            effect_uniform_buffer_count: bind_info.effect_uniform_buffer_count,
            texture_set,
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            resource_descriptor_count: bind_info.resource_descriptor_count,
            texture_count: bind_info.texture_count,
            shader_mappings: bind_info.shader_mappings.clone(),
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        })
    }
}

fn validate_effect_uniform_metadata_counts(
    pass: &SceneEffectPassGraphMaterialPass,
    bind_info: &NativeVulkanSceneEffectResourceHeapPassBindInfo,
) -> Result<(), String> {
    let count = bind_info.effect_uniform_buffer_count;
    let metadata_counts = [
        ("keys", bind_info.effect_uniforms.len()),
        (
            "buffer handles",
            bind_info.effect_uniform_buffer_handles.len(),
        ),
        (
            "device addresses",
            bind_info.effect_uniform_device_addresses.len(),
        ),
        (
            "record indices",
            bind_info.effect_uniform_record_indices.len(),
        ),
        ("bytes", bind_info.effect_uniform_bytes.len()),
        (
            "payload hashes",
            bind_info.effect_uniform_payload_hashes.len(),
        ),
    ];
    for (label, actual) in metadata_counts {
        if actual != count {
            return Err(format!(
                "scene effect resource heap bind uniform metadata count mismatch for pass {} object {:?}: {} has {}, expected {}",
                pass.graph_pass_index, pass.object, label, actual, count
            ));
        }
    }
    Ok(())
}

fn validate_iris_effect_uniform_contract(
    pass: &SceneEffectPassGraphMaterialPass,
    bind_info: &NativeVulkanSceneEffectResourceHeapPassBindInfo,
) -> Result<(), String> {
    if pass.shader.as_deref() != Some("effects/iris") {
        return Ok(());
    }
    let expected = [
        (
            NativeVulkanSceneEffectUniformStage::Vertex,
            SCENE_GPU_IRIS_EFFECT_VERTEX_UNIFORM_BYTES,
        ),
        (
            NativeVulkanSceneEffectUniformStage::Fragment,
            SCENE_GPU_IRIS_EFFECT_FRAGMENT_UNIFORM_BYTES,
        ),
    ];
    if bind_info.effect_uniform_buffer_count != expected.len() {
        return Err(format!(
            "scene effects/iris pass {} for object {:?} requires {} stage-split effect uniform buffers, got {}",
            pass.graph_pass_index,
            pass.object,
            expected.len(),
            bind_info.effect_uniform_buffer_count
        ));
    }
    for (ordinal, (expected_stage, expected_bytes)) in expected.into_iter().enumerate() {
        let uniform = bind_info.effect_uniforms.get(ordinal).ok_or_else(|| {
            format!(
                "scene effects/iris pass {} for object {:?} is missing effect uniform ordinal {ordinal}",
                pass.graph_pass_index, pass.object
            )
        })?;
        if uniform.effect_pass_index != pass.graph_pass_index
            || uniform.object != pass.object
            || uniform.shader != "effects/iris"
            || uniform.stage != expected_stage
        {
            return Err(format!(
                "scene effects/iris pass {} for object {:?} uniform ordinal {ordinal} must be {:?} for the same pass/object, got {:?}",
                pass.graph_pass_index, pass.object, expected_stage, uniform
            ));
        }
        let bytes = bind_info.effect_uniform_bytes[ordinal];
        if bytes != expected_bytes {
            return Err(format!(
                "scene effects/iris pass {} for object {:?} uniform ordinal {ordinal} has {bytes} bytes, expected {expected_bytes}",
                pass.graph_pass_index, pass.object
            ));
        }
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_resource_heap_pass_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &SceneEffectPassGraphMaterialPass,
    bind_info: NativeVulkanSceneEffectResourceHeapPassBindInfo,
) -> Result<NativeVulkanSceneEffectResourceHeapPassBindPlan, String> {
    let plan =
        NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(pass, &bind_info)?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        if let Some(sampler_bind) = bind_info.sampler_bind.as_ref() {
            device.cmd_bind_sampler_heap_ext(command_buffer, sampler_bind);
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectPassGraphOutput, SceneEffectTextureResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneResourceId, we::WeEffectKind,
    };
    use crate::renderer::native_vulkan::scene_backend::effect_uniforms::{
        NativeVulkanSceneEffectUniformKey, NativeVulkanSceneEffectUniformStage,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

    #[test]
    fn effect_resource_heap_pass_bind_plan_tracks_heap_slice_identity() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let bind_info = pass_bind_info(2, SceneObjectId(7), texture_set, 11, 3);

        let plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("effect pass resource heap bind plan");

        assert_eq!(plan.effect_pass_index, 2);
        assert_eq!(plan.object, SceneObjectId(7));
        assert_eq!(plan.heap_slice_index, 11);
        assert_eq!(plan.effect_uniform_buffer_count, 0);
        assert_eq!(plan.base_resource_descriptor_index, 3);
        assert_eq!(plan.resource_descriptor_count, 2);
        assert_eq!(plan.texture_count, 2);
        assert_eq!(
            plan.command_order,
            ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_tracks_uniform_plus_textures() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let mut bind_info = pass_bind_info(2, SceneObjectId(7), texture_set, 11, 3);
        bind_info.effect_uniforms = vec![
            NativeVulkanSceneEffectUniformKey {
                effect_pass_index: 2,
                object: SceneObjectId(7),
                shader: "effects/fluidsimulation_vorticity".to_owned(),
                stage: crate::renderer::native_vulkan::scene_backend::effect_uniforms::NativeVulkanSceneEffectUniformStage::Vertex,
            },
            NativeVulkanSceneEffectUniformKey {
                effect_pass_index: 2,
                object: SceneObjectId(7),
                shader: "effects/fluidsimulation_vorticity".to_owned(),
                stage: crate::renderer::native_vulkan::scene_backend::effect_uniforms::NativeVulkanSceneEffectUniformStage::Fragment,
            },
        ];
        bind_info.effect_uniform_buffer_count = bind_info.effect_uniforms.len();
        bind_info.effect_uniform_buffer_handles = vec![0x4200, 0x4300];
        bind_info.effect_uniform_device_addresses = vec![0x4280, 0x4380];
        bind_info.effect_uniform_record_indices = vec![0, 0];
        bind_info.effect_uniform_bytes = vec![48, 16];
        bind_info.effect_uniform_payload_hashes = vec![0x1234, 0x5678];
        bind_info.resource_descriptor_count = 4;

        let plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("effect pass resource heap bind plan");

        assert_eq!(plan.effect_uniform_buffer_count, 2);
        assert_eq!(plan.resource_descriptor_count, 4);
        assert_eq!(plan.texture_count, 2);
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_requires_iris_stage_split_uniforms() {
        let mut pass = effect_pass();
        pass.effect_file = "effects/iris/effect.json".to_owned();
        pass.effect = WeEffectKind::Iris;
        pass.shader = Some("effects/iris".to_owned());
        pass.texture_resources.clear();
        let texture_set = effect_pass_texture_set_key(&pass).expect("iris texture set");
        let bind_info = pass_bind_info(2, SceneObjectId(7), texture_set.clone(), 11, 3);

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("iris without stage-split uniform buffers must fail");

        assert!(err.contains("effects/iris"));
        assert!(err.contains("requires 2 stage-split"));

        let bind_info = pass_bind_info_with_iris_uniforms(pass_bind_info(
            2,
            SceneObjectId(7),
            texture_set,
            11,
            3,
        ));
        let plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("iris stage-split heap bind plan");

        assert_eq!(plan.effect_uniform_buffer_count, 2);
        assert_eq!(plan.resource_descriptor_count, 3);
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_rejects_uniform_count_mismatch() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let mut bind_info = pass_bind_info(2, SceneObjectId(7), texture_set, 11, 3);
        bind_info.effect_uniforms = vec![NativeVulkanSceneEffectUniformKey {
            effect_pass_index: 2,
            object: SceneObjectId(7),
            shader: "effects/fluidsimulation_vorticity".to_owned(),
            stage: crate::renderer::native_vulkan::scene_backend::effect_uniforms::NativeVulkanSceneEffectUniformStage::Vertex,
        }];
        bind_info.effect_uniform_buffer_count = 1;
        bind_info.resource_descriptor_count = 3;

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("metadata vector mismatch must fail");

        assert!(err.contains("uniform metadata count mismatch"));
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_rejects_wrong_pass_binding() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let bind_info = pass_bind_info(3, SceneObjectId(7), texture_set, 11, 3);

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("wrong pass binding must fail");

        assert!(err.contains("pass-index mismatch"));
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_rejects_texture_set_mismatch() {
        let pass = effect_pass();
        let bind_info = pass_bind_info(
            2,
            SceneObjectId(7),
            NativeVulkanSceneEffectTextureSetKey {
                bindings: vec![super::super::NativeVulkanSceneEffectTextureSetBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                        SceneResourceId(99),
                    ),
                }],
            },
            11,
            3,
        );

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("texture set mismatch must fail");

        assert!(err.contains("texture-set mismatch"));
    }

    fn effect_pass() -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 2,
            graph_pass_index: 2,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 9,
            effect_file: "effects/fluidsimulation/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some("effects/fluidsimulation_vorticity".to_owned()),
            source: Some(SceneEffectPassGraphInputBinding {
                slot: 0,
                image: crate::engine::scene_engine::SceneEffectImageRef::NamedFbo(
                    "_rt_SmokeVelocity1".to_owned(),
                ),
                source: SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(1)),
            }),
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::NamedFbo(2)),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: vec![SceneEffectTextureResourceBinding {
                slot: 2,
                resource: SceneResourceId(12),
            }],
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }

    fn pass_bind_info(
        effect_pass_index: usize,
        object: SceneObjectId,
        texture_set: NativeVulkanSceneEffectTextureSetKey,
        heap_slice_index: usize,
        base_resource_descriptor_index: usize,
    ) -> NativeVulkanSceneEffectResourceHeapPassBindInfo {
        let texture_count = texture_set.bindings.len();
        let shader_mappings = texture_set
            .bindings
            .iter()
            .enumerate()
            .map(|(ordinal, binding)| {
                format!(
                    "we.texture_slot{}.g_Texture{} -> effect-heap-slice-offset{}",
                    binding.slot, binding.slot, ordinal
                )
            })
            .collect();
        NativeVulkanSceneEffectResourceHeapPassBindInfo {
            effect_pass_index,
            object,
            heap_slice_index,
            effect_uniform_buffer_count: 0,
            effect_uniforms: Vec::new(),
            effect_uniform_buffer_handles: Vec::new(),
            effect_uniform_device_addresses: Vec::new(),
            effect_uniform_record_indices: Vec::new(),
            effect_uniform_bytes: Vec::new(),
            effect_uniform_payload_hashes: Vec::new(),
            texture_set,
            base_resource_descriptor_index,
            resource_descriptor_count: texture_count,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: Some(vk::BindHeapInfoEXT::builder().build()),
        }
    }

    fn pass_bind_info_with_iris_uniforms(
        mut bind_info: NativeVulkanSceneEffectResourceHeapPassBindInfo,
    ) -> NativeVulkanSceneEffectResourceHeapPassBindInfo {
        bind_info.effect_uniforms = vec![
            iris_uniform_key(NativeVulkanSceneEffectUniformStage::Vertex),
            iris_uniform_key(NativeVulkanSceneEffectUniformStage::Fragment),
        ];
        bind_info.effect_uniform_buffer_count = bind_info.effect_uniforms.len();
        bind_info.effect_uniform_buffer_handles = vec![0x4200, 0x4300];
        bind_info.effect_uniform_device_addresses = vec![0x4280, 0x4380];
        bind_info.effect_uniform_record_indices = vec![0, 0];
        bind_info.effect_uniform_bytes = vec![
            SCENE_GPU_IRIS_EFFECT_VERTEX_UNIFORM_BYTES,
            SCENE_GPU_IRIS_EFFECT_FRAGMENT_UNIFORM_BYTES,
        ];
        bind_info.effect_uniform_payload_hashes = vec![0x1234, 0x5678];
        bind_info.resource_descriptor_count = bind_info
            .texture_count
            .saturating_add(bind_info.effect_uniform_buffer_count);
        bind_info
    }

    fn iris_uniform_key(
        stage: NativeVulkanSceneEffectUniformStage,
    ) -> NativeVulkanSceneEffectUniformKey {
        NativeVulkanSceneEffectUniformKey {
            effect_pass_index: 2,
            object: SceneObjectId(7),
            shader: "effects/iris".to_owned(),
            stage,
        }
    }
}
