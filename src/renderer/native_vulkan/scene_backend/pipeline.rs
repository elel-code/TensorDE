//! Scene graphics pipeline key and bind command boundary.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/blending-modes.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneBlendContract, SceneGraphDraw, SceneGraphPipelineClass, SceneMaterialRenderState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct NativeVulkanScenePipelineKey<'a> {
    pub shader: &'a str,
    pub blend: SceneBlendContract,
    pub render_state: SceneMaterialRenderState,
    pub pipeline_class: SceneGraphPipelineClass,
    pub texture_slot_mask: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineBindPlan<'a> {
    pub key: NativeVulkanScenePipelineKey<'a>,
    pub command_order: [&'static str; 1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum NativeVulkanScenePipelineVertexLayout {
    SceneMeshV0,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NativeVulkanScenePipelineCacheKey {
    pub shader: String,
    pub blend: SceneBlendContract,
    pub render_state: SceneMaterialRenderState,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub target_format: vk::Format,
    pub texture_slot_mask: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanScenePipelineResources {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanScenePipelineBinding {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NativeVulkanScenePipelineCacheAction {
    Create {
        shader: String,
        target_format: String,
        vertex_layout: NativeVulkanScenePipelineVertexLayout,
    },
    Reuse {
        shader: String,
        target_format: String,
        vertex_layout: NativeVulkanScenePipelineVertexLayout,
    },
}

impl<'a> NativeVulkanScenePipelineKey<'a> {
    pub fn from_draw(draw: &'a SceneGraphDraw) -> Result<Self, String> {
        if draw.material.shader.is_empty() {
            return Err("scene pipeline key requires a non-empty WE shader name".to_owned());
        }
        Ok(Self {
            shader: draw.material.shader.as_str(),
            blend: draw.material.blend,
            render_state: draw.material.render_state,
            pipeline_class: draw.pipeline,
            texture_slot_mask: draw.shader_texture_slot_mask()?,
        })
    }
}

impl NativeVulkanScenePipelineCacheKey {
    pub fn from_bind_key(
        key: NativeVulkanScenePipelineKey<'_>,
        target_format: vk::Format,
    ) -> Result<Self, String> {
        if target_format == vk::Format::UNDEFINED {
            return Err("scene pipeline cache key requires a defined target format".to_owned());
        }
        Ok(Self {
            shader: key.shader.to_owned(),
            blend: key.blend,
            render_state: key.render_state,
            pipeline_class: key.pipeline_class,
            vertex_layout: scene_pipeline_vertex_layout(key.pipeline_class)?,
            target_format,
            texture_slot_mask: key.texture_slot_mask,
        })
    }
}

impl<'a> NativeVulkanScenePipelineBindPlan<'a> {
    pub fn from_key(key: NativeVulkanScenePipelineKey<'a>) -> Self {
        Self {
            key,
            command_order: ["cmd_bind_pipeline"],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_pipeline_bind_command<'a>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    key: NativeVulkanScenePipelineKey<'a>,
    pipeline: vk::Pipeline,
) -> Result<NativeVulkanScenePipelineBindPlan<'a>, String> {
    if pipeline == vk::Pipeline::null() {
        return Err("scene pipeline bind requires a valid vk::Pipeline".to_owned());
    }
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
    }
    Ok(NativeVulkanScenePipelineBindPlan::from_key(key))
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePipelineStore {
    pipelines: BTreeMap<NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineResources>,
    last_actions: Vec<NativeVulkanScenePipelineCacheAction>,
}

impl NativeVulkanScenePipelineStore {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            pipelines: BTreeMap::new(),
            last_actions: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn resolve_pipeline<CreatePipeline>(
        &mut self,
        key: NativeVulkanScenePipelineCacheKey,
        create_pipeline: CreatePipeline,
    ) -> Result<NativeVulkanScenePipelineBinding, String>
    where
        CreatePipeline: FnOnce(
            &NativeVulkanScenePipelineCacheKey,
        ) -> Result<NativeVulkanScenePipelineResources, String>,
    {
        self.last_actions.clear();
        if let Some(resources) = self.pipelines.get(&key) {
            self.last_actions
                .push(scene_pipeline_cache_action_reuse(&key));
            return Ok(pipeline_binding(*resources));
        }

        let resources = create_pipeline(&key)?;
        validate_scene_pipeline_resources(resources)?;
        self.pipelines.insert(key.clone(), resources);
        self.last_actions
            .push(scene_pipeline_cache_action_create(&key));
        Ok(pipeline_binding(resources))
    }

    pub(in crate::renderer::native_vulkan) fn cached_pipeline(
        &self,
        key: &NativeVulkanScenePipelineCacheKey,
    ) -> Option<NativeVulkanScenePipelineBinding> {
        self.pipelines.get(key).copied().map(pipeline_binding)
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        for (_, resources) in std::mem::take(&mut self.pipelines) {
            unsafe {
                device.destroy_pipeline(resources.pipeline, None);
                if resources.pipeline_layout != vk::PipelineLayout::null() {
                    device.destroy_pipeline_layout(resources.pipeline_layout, None);
                }
            }
        }
        self.last_actions.clear();
    }
}

impl Default for NativeVulkanScenePipelineStore {
    fn default() -> Self {
        Self::new()
    }
}

fn scene_pipeline_vertex_layout(
    pipeline_class: SceneGraphPipelineClass,
) -> Result<NativeVulkanScenePipelineVertexLayout, String> {
    match pipeline_class {
        SceneGraphPipelineClass::Mesh => Ok(NativeVulkanScenePipelineVertexLayout::SceneMeshV0),
        SceneGraphPipelineClass::Quad
        | SceneGraphPipelineClass::PuppetSkinning
        | SceneGraphPipelineClass::ParticleEmitter => Err(format!(
            "scene pipeline cache does not support {:?} pipeline yet",
            pipeline_class
        )),
    }
}

fn validate_scene_pipeline_resources(
    resources: NativeVulkanScenePipelineResources,
) -> Result<(), String> {
    if resources.pipeline == vk::Pipeline::null() {
        return Err("scene pipeline cache requires a valid vk::Pipeline".to_owned());
    }
    Ok(())
}

fn pipeline_binding(
    resources: NativeVulkanScenePipelineResources,
) -> NativeVulkanScenePipelineBinding {
    NativeVulkanScenePipelineBinding {
        pipeline: resources.pipeline,
        pipeline_layout: resources.pipeline_layout,
    }
}

fn scene_pipeline_cache_action_create(
    key: &NativeVulkanScenePipelineCacheKey,
) -> NativeVulkanScenePipelineCacheAction {
    NativeVulkanScenePipelineCacheAction::Create {
        shader: key.shader.clone(),
        target_format: format!("{:?}", key.target_format),
        vertex_layout: key.vertex_layout,
    }
}

fn scene_pipeline_cache_action_reuse(
    key: &NativeVulkanScenePipelineCacheKey,
) -> NativeVulkanScenePipelineCacheAction {
    NativeVulkanScenePipelineCacheAction::Reuse {
        shader: key.shader.clone(),
        target_format: format!("{:?}", key.target_format),
        vertex_layout: key.vertex_layout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneMaterialKey, SceneObjectId, SceneResourceId,
    };

    #[test]
    fn pipeline_key_borrows_draw_material_without_shader_clone() {
        let draw = mesh_draw("we/genericimage4");

        let key = NativeVulkanScenePipelineKey::from_draw(&draw).unwrap();

        assert_eq!(key.shader, "we/genericimage4");
        assert_eq!(key.shader.as_ptr(), draw.material.shader.as_ptr());
        assert_eq!(key.blend, SceneBlendContract::TranslucentAlpha);
        assert_eq!(key.pipeline_class, SceneGraphPipelineClass::Mesh);
        assert_eq!(key.texture_slot_mask, 1);
    }

    #[test]
    fn pipeline_key_rejects_empty_shader() {
        let draw = mesh_draw("");

        let err = NativeVulkanScenePipelineKey::from_draw(&draw)
            .expect_err("empty WE shader name must fail");

        assert!(err.contains("non-empty WE shader name"));
    }

    #[test]
    fn pipeline_cache_key_includes_target_format_and_vertex_layout() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineKey::from_draw(&draw).unwrap();

        let cache_key =
            NativeVulkanScenePipelineCacheKey::from_bind_key(key, vk::Format::B8G8R8A8_UNORM)
                .expect("mesh pipeline cache key");

        assert_eq!(cache_key.shader, "we/genericimage4");
        assert_eq!(cache_key.target_format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(cache_key.texture_slot_mask, 1);
        assert_eq!(
            cache_key.vertex_layout,
            NativeVulkanScenePipelineVertexLayout::SceneMeshV0
        );
    }

    #[test]
    fn pipeline_cache_key_rejects_undefined_target_format() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineKey::from_draw(&draw).unwrap();

        let err = NativeVulkanScenePipelineCacheKey::from_bind_key(key, vk::Format::UNDEFINED)
            .expect_err("undefined target format must fail");

        assert!(err.contains("defined target format"));
    }

    #[test]
    fn pipeline_store_creates_once_and_reuses_by_full_cache_key() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineCacheKey::from_bind_key(
            NativeVulkanScenePipelineKey::from_draw(&draw).unwrap(),
            vk::Format::B8G8R8A8_UNORM,
        )
        .unwrap();
        let mut store = NativeVulkanScenePipelineStore::new();
        let mut creates = 0usize;

        let first = store
            .resolve_pipeline(key.clone(), |_| {
                creates += 1;
                Ok(pipeline_resources(11, 12))
            })
            .expect("create pipeline");
        let second = store
            .resolve_pipeline(key, |_| {
                creates += 1;
                Ok(pipeline_resources(21, 22))
            })
            .expect("reuse pipeline");

        assert_eq!(creates, 1);
        assert_eq!(store.len(), 1);
        assert_eq!(first.pipeline, second.pipeline);
        assert!(matches!(
            store.last_actions(),
            [NativeVulkanScenePipelineCacheAction::Reuse { .. }]
        ));
    }

    #[test]
    fn pipeline_store_separates_target_formats() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineKey::from_draw(&draw).unwrap();
        let first_key =
            NativeVulkanScenePipelineCacheKey::from_bind_key(key, vk::Format::B8G8R8A8_UNORM)
                .unwrap();
        let second_key =
            NativeVulkanScenePipelineCacheKey::from_bind_key(key, vk::Format::R8G8B8A8_UNORM)
                .unwrap();
        let mut store = NativeVulkanScenePipelineStore::new();

        let first = store
            .resolve_pipeline(first_key, |_| Ok(pipeline_resources(11, 12)))
            .expect("first target format pipeline");
        let second = store
            .resolve_pipeline(second_key, |_| Ok(pipeline_resources(21, 22)))
            .expect("second target format pipeline");

        assert_eq!(store.len(), 2);
        assert_ne!(first.pipeline, second.pipeline);
    }

    #[test]
    fn pipeline_store_separates_we_texture_slot_interfaces() {
        let slot0 = NativeVulkanScenePipelineCacheKey::from_bind_key(
            NativeVulkanScenePipelineKey::from_draw(&mesh_draw_with_resources(
                "we/genericimage4",
                vec![SceneGraphResourceBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    resource: SceneResourceId(7),
                }],
            ))
            .unwrap(),
            vk::Format::B8G8R8A8_UNORM,
        )
        .unwrap();
        let slot0_and_4 = NativeVulkanScenePipelineCacheKey::from_bind_key(
            NativeVulkanScenePipelineKey::from_draw(&mesh_draw_with_resources(
                "we/genericimage4",
                vec![
                    SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(7),
                    },
                    SceneGraphResourceBinding {
                        slot: 4,
                        role: SceneGraphResourceRole::shader_texture(4),
                        resource: SceneResourceId(8),
                    },
                ],
            ))
            .unwrap(),
            vk::Format::B8G8R8A8_UNORM,
        )
        .unwrap();
        let mut store = NativeVulkanScenePipelineStore::new();

        let first = store
            .resolve_pipeline(slot0, |_| Ok(pipeline_resources(11, 12)))
            .expect("slot0 pipeline");
        let second = store
            .resolve_pipeline(slot0_and_4, |_| Ok(pipeline_resources(21, 22)))
            .expect("slot0+4 pipeline");

        assert_eq!(store.len(), 2);
        assert_ne!(first.pipeline, second.pipeline);
    }

    #[test]
    fn pipeline_store_rejects_null_pipeline_from_factory() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineCacheKey::from_bind_key(
            NativeVulkanScenePipelineKey::from_draw(&draw).unwrap(),
            vk::Format::B8G8R8A8_UNORM,
        )
        .unwrap();
        let mut store = NativeVulkanScenePipelineStore::new();

        let err = store
            .resolve_pipeline(key, |_| Ok(pipeline_resources(0, 12)))
            .expect_err("null pipeline must fail");

        assert!(err.contains("valid vk::Pipeline"));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn pipeline_store_exposes_cached_pipeline_without_factory_call() {
        let draw = mesh_draw("we/genericimage4");
        let key = NativeVulkanScenePipelineCacheKey::from_bind_key(
            NativeVulkanScenePipelineKey::from_draw(&draw).unwrap(),
            vk::Format::B8G8R8A8_UNORM,
        )
        .unwrap();
        let mut store = NativeVulkanScenePipelineStore::new();
        store
            .resolve_pipeline(key.clone(), |_| Ok(pipeline_resources(11, 12)))
            .expect("create pipeline");

        let binding = store
            .cached_pipeline(&key)
            .expect("cached pipeline after warmup");

        assert_eq!(binding.pipeline, vk::Pipeline::from_raw(11));
        assert_eq!(binding.pipeline_layout, vk::PipelineLayout::from_raw(12));
    }

    fn mesh_draw(shader: &str) -> SceneGraphDraw {
        mesh_draw_with_resources(
            shader,
            vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(7),
            }],
        )
    }

    fn mesh_draw_with_resources(
        shader: &str,
        resources: Vec<SceneGraphResourceBinding>,
    ) -> SceneGraphDraw {
        SceneGraphDraw {
            object: SceneObjectId(2),
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: shader.to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(4)),
            puppet: None,
            resources,
            index_count: 6,
        }
    }

    fn pipeline_resources(
        pipeline: u64,
        pipeline_layout: u64,
    ) -> NativeVulkanScenePipelineResources {
        NativeVulkanScenePipelineResources {
            pipeline: vk::Pipeline::from_raw(pipeline),
            pipeline_layout: vk::PipelineLayout::from_raw(pipeline_layout),
        }
    }
}
