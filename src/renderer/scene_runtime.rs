mod input;

#[cfg(test)]
pub(super) use self::input::scene_audio_response_property_value;
pub(super) use self::input::{
    scene_input_properties_from_sources, scene_property_value, scene_render_property_value,
    scene_runtime_property_value_with_inputs, scene_runtime_text_property_value_with_inputs,
};

use crate::core::scene::{SceneSnapshotLayer, SceneSnapshotSampledImageLayer};
use crate::core::{FitMode, SceneDocument, SceneSize};
use crate::renderer::{
    RendererPlanError, SceneRenderLayer, SceneWallpaperPlan, load_scene_document,
    scene_bound_properties, scene_default_gscene_package_root, scene_display_plan,
    scene_plan_system_metrics, scene_render_layers_from_snapshot,
    scene_render_layers_from_snapshot_into, scene_timeline_animated_layer_count,
    scene_timeline_animation_count,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SceneWallpaperRuntimeSampler {
    output_name: String,
    package_root: PathBuf,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    scene_fit: FitMode,
    cursor_parallax_input_ready: bool,
    input_properties: BTreeMap<String, Value>,
    document: SceneDocument,
    snapshot_layers_scratch: Vec<SceneSnapshotLayer>,
    sampled_image_layers_scratch: Vec<SceneSnapshotSampledImageLayer>,
    render_layers_scratch: Vec<SceneRenderLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneWallpaperRuntimeFrame {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub layers: Vec<SceneRenderLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneWallpaperRuntimeSnapshotFrame {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub layers: Vec<SceneSnapshotLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneWallpaperRuntimeSampledImageFrame {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub layers: Vec<SceneSnapshotSampledImageLayer>,
}

impl SceneWallpaperRuntimeSampler {
    pub fn from_plan(plan: &SceneWallpaperPlan) -> Result<Option<Self>, RendererPlanError> {
        let Some(source_path) = plan.source.clone() else {
            return Ok(None);
        };
        let document = load_scene_document(&source_path)?;
        Ok(Some(Self {
            output_name: plan.output_name.clone(),
            package_root: scene_default_gscene_package_root(&source_path),
            source_path,
            target_max_fps: plan.target_max_fps,
            scene_fit: plan.scene_fit,
            cursor_parallax_input_ready: plan.cursor_parallax_input_ready,
            input_properties: plan.scene_input_properties.clone(),
            document,
            snapshot_layers_scratch: Vec::new(),
            sampled_image_layers_scratch: Vec::new(),
            render_layers_scratch: Vec::new(),
        }))
    }

    pub fn sample_frame(
        &self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        let snapshot = self.document.snapshot_at_with_resolvers(
            time_ms,
            |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
            },
            |property| {
                scene_runtime_text_property_value_with_inputs(property, &self.input_properties)
            },
        );
        let layers =
            scene_render_layers_from_snapshot(&self.package_root, &self.document, snapshot.layers)?;
        Ok(SceneWallpaperRuntimeFrame {
            snapshot_time_ms: snapshot.time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers,
        })
    }

    pub fn sample_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        self.document.snapshot_compact_layers_at_with_resolvers(
            time_ms,
            |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
            },
            |property| {
                scene_runtime_text_property_value_with_inputs(property, &self.input_properties)
            },
            &mut self.snapshot_layers_scratch,
        );
        scene_render_layers_from_snapshot_into(
            &self.package_root,
            &self.document,
            &mut self.snapshot_layers_scratch,
            &mut self.render_layers_scratch,
        )?;
        Ok(SceneWallpaperRuntimeFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.render_layers_scratch),
        })
    }

    pub fn recycle_frame(&mut self, mut frame: SceneWallpaperRuntimeFrame) {
        frame.layers.clear();
        self.render_layers_scratch = frame.layers;
    }

    pub fn package_root(&self) -> &Path {
        &self.package_root
    }

    pub fn sample_snapshot_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeSnapshotFrame, RendererPlanError> {
        self.document.snapshot_compact_layers_at_with_resolvers(
            time_ms,
            |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
            },
            |property| {
                scene_runtime_text_property_value_with_inputs(property, &self.input_properties)
            },
            &mut self.snapshot_layers_scratch,
        );
        Ok(SceneWallpaperRuntimeSnapshotFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.snapshot_layers_scratch),
        })
    }

    pub fn recycle_snapshot_frame(&mut self, mut frame: SceneWallpaperRuntimeSnapshotFrame) {
        frame.layers.clear();
        self.snapshot_layers_scratch = frame.layers;
    }

    pub fn sample_solid_snapshot_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeSnapshotFrame, RendererPlanError> {
        self.document.snapshot_solid_layers_at_with_resolvers(
            time_ms,
            |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
            },
            |property| {
                scene_runtime_text_property_value_with_inputs(property, &self.input_properties)
            },
            &mut self.snapshot_layers_scratch,
        );
        Ok(SceneWallpaperRuntimeSnapshotFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.snapshot_layers_scratch),
        })
    }

    pub fn sample_sampled_image_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeSampledImageFrame, RendererPlanError> {
        self.document
            .snapshot_sampled_image_layers_at_with_resolvers(
                time_ms,
                |property| {
                    scene_runtime_property_value_with_inputs(
                        &self.document,
                        time_ms,
                        property,
                        &self.input_properties,
                    )
                },
                &mut self.sampled_image_layers_scratch,
            );
        Ok(SceneWallpaperRuntimeSampledImageFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.sampled_image_layers_scratch),
        })
    }

    pub fn recycle_sampled_image_frame(
        &mut self,
        mut frame: SceneWallpaperRuntimeSampledImageFrame,
    ) {
        frame.layers.clear();
        self.sampled_image_layers_scratch = frame.layers;
    }

    pub fn dynamic_solid_geometry_required(&self) -> bool {
        self.document.dynamic_solid_geometry_required()
    }

    pub fn sample_plan(&self, time_ms: u64) -> Result<SceneWallpaperPlan, RendererPlanError> {
        let frame = self.sample_frame(time_ms)?;
        let system_metrics = scene_plan_system_metrics(&self.document);
        let display = scene_display_plan(
            Some(self.source_path.as_path()),
            &self.document,
            &frame.layers,
            Some(self.scene_fit),
            None,
            None,
        );
        Ok(SceneWallpaperPlan {
            output_name: self.output_name.clone(),
            source: Some(self.source_path.clone()),
            manifest_max_fps: None,
            target_max_fps: self.target_max_fps,
            snapshot_time_ms: frame.snapshot_time_ms,
            scene_size: frame.scene_size,
            scene_fit: frame.scene_fit,
            scene_systems: self.document.systems.clone(),
            audio_cue_count: frame.layers.iter().map(|layer| layer.audio.len()).sum(),
            bound_properties: scene_bound_properties(&self.document),
            timeline_animation_count: scene_timeline_animation_count(&self.document),
            timeline_animated_layer_count: scene_timeline_animated_layer_count(&self.document),
            property_binding_count: self.document.property_bindings.len(),
            cursor_parallax_input_ready: self.cursor_parallax_input_ready,
            scene_input_properties: self.input_properties.clone(),
            scene_scenescript_binding_count: system_metrics.scenescript_binding_count,
            scene_material_graph_count: system_metrics.material_graph_count,
            scene_material_graph_resource_count: system_metrics.material_graph_resource_count,
            scene_effect_graph_count: system_metrics.effect_graph_count,
            scene_audio_response_binding_count: system_metrics.audio_response_binding_count,
            unsupported_scene_features: system_metrics.unsupported_features,
            display,
            layers: frame.layers,
        })
    }
}
