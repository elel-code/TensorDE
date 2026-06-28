use crate::core::manifest::PropertySpec;
use crate::core::scene::SceneSnapshotLayer;
use crate::core::{FitMode, SceneDocument, SceneNode, SceneSize, SceneSystemStatus};
use crate::renderer::{
    RendererPlanError, SceneRenderLayer, SceneWallpaperPlan, load_scene_document,
    scene_bound_properties, scene_default_gscene_package_root, scene_display_plan,
    scene_plan_system_metrics, scene_render_layers_from_snapshot,
    scene_render_layers_from_snapshot_into, scene_timeline_animated_layer_count,
    scene_timeline_animation_count,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

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
    render_layers_scratch: Vec<SceneRenderLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneWallpaperRuntimeFrame {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub layers: Vec<SceneRenderLayer>,
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
            render_layers_scratch: Vec::new(),
        }))
    }

    pub fn sample_frame(
        &self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        let snapshot = self
            .document
            .snapshot_at_with_property_resolver(time_ms, |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
            });
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
        self.document.snapshot_layers_at_with_property_resolver(
            time_ms,
            |property| {
                scene_runtime_property_value_with_inputs(
                    &self.document,
                    time_ms,
                    property,
                    &self.input_properties,
                )
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

pub(super) fn scene_render_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Option<f64> {
    render_properties
        .and_then(|properties| properties.get(property))
        .and_then(scene_runtime_number)
}

pub(super) fn scene_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: &BTreeMap<String, PropertySpec>,
) -> Option<f64> {
    scene_render_property_value(property, render_properties).or_else(|| {
        manifest_properties
            .get(property)
            .and_then(scene_manifest_property_default_number)
    })
}

pub(super) fn scene_input_properties_from_sources(
    document: &SceneDocument,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: Option<&BTreeMap<String, PropertySpec>>,
) -> BTreeMap<String, Value> {
    let mut properties = BTreeMap::new();
    for binding in &document.property_bindings {
        if let Some(value) = render_properties.and_then(|source| source.get(&binding.property))
            && scene_runtime_number(value).is_some()
        {
            properties.insert(binding.property.clone(), value.clone());
            continue;
        }
        if let Some(default) = manifest_properties
            .and_then(|source| source.get(&binding.property))
            .and_then(scene_manifest_property_default_number)
        {
            properties.insert(binding.property.clone(), Value::from(default));
        }
    }

    if let Some(render_properties) = render_properties {
        for property in ["scene.parallax.x", "scene.parallax.y"] {
            if let Some(value) = render_properties.get(property)
                && scene_runtime_number(value).is_some()
            {
                properties.insert(property.to_owned(), value.clone());
            }
        }
        for node in &document.nodes {
            scene_collect_controller_input_properties(node, render_properties, &mut properties);
        }
    }

    properties
}

fn scene_collect_controller_input_properties(
    node: &SceneNode,
    render_properties: &BTreeMap<String, Value>,
    output: &mut BTreeMap<String, Value>,
) {
    if let Some(controller) = node.properties.get("controller").and_then(Value::as_object)
        && controller
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime == "native")
        && let Some(property) = controller.get("property").and_then(Value::as_str)
    {
        let property = property.trim();
        for alias in scene_controller_input_aliases(node, controller, property) {
            if let Some(value) = render_properties.get(&alias)
                && scene_runtime_number(value).is_some()
            {
                output.insert(property.to_owned(), value.clone());
                break;
            }
        }
    }
    for child in &node.children {
        scene_collect_controller_input_properties(child, render_properties, output);
    }
}

fn scene_controller_input_aliases(
    node: &SceneNode,
    controller: &serde_json::Map<String, Value>,
    property: &str,
) -> Vec<String> {
    let mut aliases = vec![
        property.to_owned(),
        format!("scene.input.{}.active", node.id),
        format!("scene.input.controller.{}.active", node.id),
    ];
    if let Some(target_node) = controller.get("target_node").and_then(Value::as_str) {
        aliases.push(format!("scene.input.{target_node}.active"));
        aliases.push(format!("scene.input.controller.{target_node}.active"));
    }
    aliases
}

pub(super) fn scene_runtime_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    scene_controller_property_value(document, time_ms, property)
        .or_else(|| scene_audio_response_property_value(document, time_ms, property))
}

pub(super) fn scene_runtime_property_value_with_inputs(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
    input_properties: &BTreeMap<String, Value>,
) -> Option<f64> {
    scene_runtime_input_property_value(input_properties, property)
        .or_else(|| scene_runtime_property_value(document, time_ms, property))
}

fn scene_runtime_input_property_value(
    input_properties: &BTreeMap<String, Value>,
    property: &str,
) -> Option<f64> {
    input_properties
        .get(property.trim())
        .and_then(scene_runtime_number)
}

fn scene_controller_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    let property = property.trim();
    if !property.starts_with("scene.controller.") {
        return None;
    }
    document
        .nodes
        .iter()
        .find_map(|node| scene_node_controller_property_value(node, time_ms, property))
}

fn scene_node_controller_property_value(
    node: &SceneNode,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if let Some(controller) = node.properties.get("controller").and_then(Value::as_object)
        && controller
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime == "native")
        && controller
            .get("property")
            .and_then(Value::as_str)
            .is_some_and(|controller_property| controller_property.trim() == property)
    {
        match controller.get("kind").and_then(Value::as_str)? {
            "idle-video-switch" => {
                let inactive_sec = controller
                    .get("mouse_inactive_sec")
                    .and_then(scene_runtime_config_number)?;
                let inactive_ms = (inactive_sec.max(0.0) * 1000.0).round();
                if (time_ms as f64) < inactive_ms {
                    return Some(0.0);
                }
                let fade_in_ms = controller
                    .get("fade_in_duration")
                    .and_then(scene_runtime_config_number)
                    .unwrap_or(0.0)
                    .max(0.0)
                    * 1000.0;
                if fade_in_ms <= 0.0 {
                    return Some(1.0);
                }
                return Some(((time_ms as f64 - inactive_ms) / fade_in_ms).clamp(0.0, 1.0));
            }
            "click-video-switch" => return Some(0.0),
            _ => return None,
        }
    }
    node.children
        .iter()
        .find_map(|child| scene_node_controller_property_value(child, time_ms, property))
}

pub(super) fn scene_audio_response_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if document.systems.audio_response != SceneSystemStatus::Ready {
        return None;
    }
    let property = property
        .trim()
        .replace(['-', ' ', '/'], "_")
        .to_ascii_lowercase();
    if property.is_empty() {
        return None;
    }
    let band = if property == "audio"
        || property == "audio_level"
        || property == "audio_response"
        || property.ends_with("_audio")
    {
        "full"
    } else if property.contains("bass") || property.contains("low") {
        "bass"
    } else if property.contains("mid") || property.contains("vocal") {
        "mid"
    } else if property.contains("treble") || property.contains("high") {
        "treble"
    } else if property.contains("spectrum") || property.contains("frequency") {
        "spectrum"
    } else {
        return None;
    };
    let seconds = time_ms as f64 / 1000.0;
    let (frequency, phase, floor, gain) = match band {
        "bass" => (1.25, 0.0, 0.12, 0.88),
        "mid" => (2.5, 0.7, 0.08, 0.78),
        "treble" => (5.0, 1.3, 0.04, 0.72),
        "spectrum" => (
            3.5,
            scene_audio_response_spectrum_phase(&property),
            0.05,
            0.8,
        ),
        _ => (1.75, 0.35, 0.1, 0.82),
    };
    let wave = (seconds.mul_add(frequency * std::f64::consts::TAU, phase)).sin() * 0.5 + 0.5;
    Some((floor + wave.powf(1.35) * gain).clamp(0.0, 1.0))
}

fn scene_audio_response_spectrum_phase(property: &str) -> f64 {
    let bin = property
        .rsplit('_')
        .find_map(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    f64::from(bin % 32) * 0.196_349_540_849_362_07
}

fn scene_runtime_config_number(value: &Value) -> Option<f64> {
    scene_runtime_number(value.get("value").unwrap_or(value))
}

fn scene_runtime_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

fn scene_manifest_property_default_number(property: &PropertySpec) -> Option<f64> {
    let number = match property {
        PropertySpec::Bool { default } => {
            if (*default)? {
                1.0
            } else {
                0.0
            }
        }
        PropertySpec::Number { default } | PropertySpec::Range { default, .. } => (*default)?,
        PropertySpec::Choice { .. }
        | PropertySpec::Color { .. }
        | PropertySpec::Text { .. }
        | PropertySpec::File { .. } => return None,
    };
    number.is_finite().then_some(number)
}
