use super::manifest::FitMode;
use super::path::PackagePath;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const SCENE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneDocument {
    #[serde(default = "default_scene_version")]
    pub version: u32,
    #[serde(default)]
    pub profile: SceneProfile,
    #[serde(default)]
    pub source: SceneSourceMetadata,
    #[serde(default)]
    pub size: Option<SceneSize>,
    #[serde(default)]
    pub resources: Vec<SceneResource>,
    #[serde(default)]
    pub nodes: Vec<SceneNode>,
    #[serde(default)]
    pub timelines: Vec<SceneTimeline>,
    #[serde(default)]
    pub property_bindings: Vec<ScenePropertyBinding>,
    #[serde(default)]
    pub systems: SceneSystems,
    #[serde(default)]
    pub native_lowering: SceneNativeLowering,
    #[serde(default)]
    pub unsupported_features: Vec<SceneUnsupportedFeature>,
}

impl SceneDocument {
    pub fn validate(&self) -> Result<(), SceneError> {
        if self.version != SCENE_VERSION {
            return Err(SceneError::invalid(format!(
                "unsupported scene version {}; supported version is {}",
                self.version, SCENE_VERSION
            )));
        }
        if let Some(size) = self.size {
            size.validate()?;
        }

        let mut resource_ids = BTreeSet::new();
        for resource in &self.resources {
            resource.validate(&mut resource_ids)?;
        }

        let mut node_ids = BTreeSet::new();
        for node in &self.nodes {
            node.validate(&resource_ids, &mut node_ids)?;
        }
        for timeline in &self.timelines {
            timeline.validate(&node_ids)?;
        }
        for binding in &self.property_bindings {
            binding.validate(&node_ids)?;
        }
        for feature in &self.unsupported_features {
            feature.validate()?;
        }
        Ok(())
    }

    pub fn referenced_paths(&self) -> Vec<PackagePath> {
        let mut paths = Vec::new();
        if let Some(path) = &self.source.metadata {
            paths.push(path.clone());
        }
        for resource in &self.resources {
            paths.push(resource.source.clone());
        }
        if let Some(fallback) = &self.native_lowering.fallback {
            paths.push(fallback.clone());
        }
        paths
    }

    pub fn snapshot_at_with_property_resolver<F>(
        &self,
        time_ms: u64,
        resolve_property: F,
    ) -> SceneSnapshot
    where
        F: Fn(&str) -> Option<f64>,
    {
        let resources = self
            .resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource))
            .collect::<BTreeMap<_, _>>();
        let mut layers = Vec::new();
        for node in &self.nodes {
            node.push_snapshot_layers(
                time_ms,
                SceneTransform::default(),
                1.0,
                &resources,
                &self.timelines,
                &self.property_bindings,
                &resolve_property,
                &mut layers,
            );
        }
        SceneSnapshot { time_ms, layers }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneProfile {
    #[default]
    NativeVulkanFullScene,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSourceMetadata {
    pub format: Option<String>,
    pub metadata: Option<PackagePath>,
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSize {
    pub width: u32,
    pub height: u32,
}

impl SceneSize {
    fn validate(self) -> Result<(), SceneError> {
        if self.width == 0 || self.height == 0 {
            return Err(SceneError::invalid(
                "scene size width and height must be greater than 0",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneResource {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: SceneResourceKind,
    pub source: PackagePath,
    #[serde(default)]
    pub original_source: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl SceneResource {
    fn validate(&self, resource_ids: &mut BTreeSet<String>) -> Result<(), SceneError> {
        validate_required_text("scene resource id", &self.id)?;
        if !resource_ids.insert(self.id.clone()) {
            return Err(SceneError::invalid(format!(
                "duplicate scene resource id {:?}",
                self.id
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneResourceKind {
    Image,
    Video,
    Audio,
    Shader,
    Script,
    Json,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: SceneNodeKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub transform: SceneTransform,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub stroke_color: Option<String>,
    #[serde(default)]
    pub stroke_width: Option<f64>,
    #[serde(default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub text_align: Option<SceneTextAlign>,
    #[serde(default)]
    #[serde(rename = "path")]
    pub path_data: Option<String>,
    #[serde(default)]
    pub fit: FitMode,
    #[serde(default)]
    pub original_type: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    fn validate(
        &self,
        resource_ids: &BTreeSet<String>,
        node_ids: &mut BTreeSet<String>,
    ) -> Result<(), SceneError> {
        validate_required_text("scene node id", &self.id)?;
        if !node_ids.insert(self.id.clone()) {
            return Err(SceneError::invalid(format!(
                "duplicate scene node id {:?}",
                self.id
            )));
        }
        validate_opacity(self.opacity, &self.id)?;
        self.transform.validate(&self.id)?;
        if let Some(resource) = &self.resource
            && !resource_ids.contains(resource)
        {
            return Err(SceneError::invalid(format!(
                "scene node {:?} references unknown resource {:?}",
                self.id, resource
            )));
        }
        for child in &self.children {
            child.validate(resource_ids, node_ids)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_snapshot_layers(
        &self,
        time_ms: u64,
        parent_transform: SceneTransform,
        parent_opacity: f64,
        resources: &BTreeMap<&str, &SceneResource>,
        timelines: &[SceneTimeline],
        property_bindings: &[ScenePropertyBinding],
        resolve_property: &impl Fn(&str) -> Option<f64>,
        output: &mut Vec<SceneSnapshotLayer>,
    ) {
        if !self.visible {
            return;
        }
        let mut transform = self.transform;
        let mut opacity = self.opacity;
        for timeline in timelines
            .iter()
            .filter(|timeline| timeline.target_node.as_deref() == Some(self.id.as_str()))
        {
            for channel in &timeline.channels {
                let value = channel.value_at(time_ms);
                apply_scene_animated_value(&mut transform, &mut opacity, channel.property, value);
            }
        }
        for binding in property_bindings.iter().filter(|binding| {
            binding
                .target_node
                .as_deref()
                .is_none_or(|target| target == self.id)
        }) {
            let Some(raw_value) = resolve_property(&binding.property) else {
                continue;
            };
            let value = raw_value * binding.scale.unwrap_or(1.0) + binding.offset.unwrap_or(0.0);
            if value.is_finite() {
                apply_scene_animated_value(&mut transform, &mut opacity, binding.target, value);
            }
        }

        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        if self.kind != SceneNodeKind::Group {
            output.push(SceneSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: self
                    .resource
                    .as_deref()
                    .and_then(|resource| resources.get(resource))
                    .map(|resource| resource.source.clone()),
                color: self.color.clone(),
                stroke_color: self.stroke_color.clone(),
                stroke_width: self.stroke_width,
                corner_radius: self.corner_radius,
                width: self.width,
                height: self.height,
                text: self.text.clone(),
                font_size: self.font_size,
                font_family: self.font_family.clone(),
                font_weight: self.font_weight.clone(),
                text_align: self.text_align,
                path_data: self.path_data.clone(),
                fit: self.fit,
                opacity,
                transform,
            });
        }
        for child in &self.children {
            child.push_snapshot_layers(
                time_ms,
                transform,
                opacity,
                resources,
                timelines,
                property_bindings,
                resolve_property,
                output,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneNodeKind {
    Image,
    Video,
    Color,
    Rectangle,
    Ellipse,
    Text,
    Path,
    Group,
    Shader,
    ParticleEmitter,
    AudioResponse,
    Script,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextAlign {
    #[default]
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTransform {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_scale")]
    pub scale_x: f64,
    #[serde(default = "default_scale")]
    pub scale_y: f64,
    #[serde(default)]
    pub rotation_deg: f64,
    #[serde(default = "default_anchor")]
    pub anchor_x: f64,
    #[serde(default = "default_anchor")]
    pub anchor_y: f64,
}

impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            anchor_x: 0.5,
            anchor_y: 0.5,
        }
    }
}

impl SceneTransform {
    fn validate(self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("x", self.x),
            ("y", self.y),
            ("scale_x", self.scale_x),
            ("scale_y", self.scale_y),
            ("rotation_deg", self.rotation_deg),
            ("anchor_x", self.anchor_x),
            ("anchor_y", self.anchor_y),
        ] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} transform {field} must be finite"
                )));
            }
        }
        if self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} transform scale values must be greater than 0"
            )));
        }
        Ok(())
    }

    fn compose(self, child: Self) -> Self {
        Self {
            x: self.x + child.x * self.scale_x,
            y: self.y + child.y * self.scale_y,
            scale_x: self.scale_x * child.scale_x,
            scale_y: self.scale_y * child.scale_y,
            rotation_deg: self.rotation_deg + child.rotation_deg,
            anchor_x: child.anchor_x,
            anchor_y: child.anchor_y,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTimeline {
    pub id: String,
    #[serde(default)]
    pub target_node: Option<String>,
    #[serde(default)]
    pub channels: Vec<SceneTimelineChannel>,
}

impl SceneTimeline {
    fn validate(&self, node_ids: &BTreeSet<String>) -> Result<(), SceneError> {
        validate_required_text("scene timeline id", &self.id)?;
        if let Some(target_node) = &self.target_node
            && !node_ids.contains(target_node)
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {:?} references unknown target node {:?}",
                self.id, target_node
            )));
        }
        for channel in &self.channels {
            channel.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTimelineChannel {
    pub property: SceneAnimatedProperty,
    #[serde(rename = "loop", default)]
    pub loop_playback: bool,
    #[serde(default)]
    pub keyframes: Vec<SceneKeyframe>,
}

impl SceneTimelineChannel {
    fn validate(&self) -> Result<(), SceneError> {
        for keyframe in &self.keyframes {
            keyframe.validate(self.property)?;
        }
        Ok(())
    }

    fn value_at(&self, time_ms: u64) -> f64 {
        let Some(first) = self.keyframes.first() else {
            return 0.0;
        };
        if self.keyframes.len() == 1 {
            return first.value;
        }
        let last_time = self
            .keyframes
            .last()
            .map(|keyframe| keyframe.time_ms)
            .unwrap_or_default();
        let time_ms = if self.loop_playback && last_time > 0 {
            time_ms % last_time
        } else {
            time_ms
        };
        if time_ms <= first.time_ms {
            return first.value;
        }
        for pair in self.keyframes.windows(2) {
            let start = &pair[0];
            let end = &pair[1];
            if time_ms <= end.time_ms {
                let span = (end.time_ms - start.time_ms) as f64;
                let progress = if span > 0.0 {
                    (time_ms - start.time_ms) as f64 / span
                } else {
                    1.0
                };
                let eased = end.curve.ease(progress.clamp(0.0, 1.0));
                return start.value + (end.value - start.value) * eased;
            }
        }
        self.keyframes
            .last()
            .map(|keyframe| keyframe.value)
            .unwrap_or(first.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAnimatedProperty {
    Opacity,
    X,
    Y,
    ScaleX,
    ScaleY,
    RotationDeg,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneKeyframe {
    pub time_ms: u64,
    pub value: f64,
    #[serde(default)]
    pub curve: SceneCurve,
}

impl SceneKeyframe {
    fn validate(self, property: SceneAnimatedProperty) -> Result<(), SceneError> {
        if !self.value.is_finite() {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} keyframe value must be finite"
            )));
        }
        if property == SceneAnimatedProperty::Opacity {
            validate_opacity(self.value, "timeline")?;
        }
        if matches!(
            property,
            SceneAnimatedProperty::ScaleX | SceneAnimatedProperty::ScaleY
        ) && self.value <= 0.0
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} scale value must be greater than 0"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneCurve {
    #[default]
    Linear,
    Step,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl SceneCurve {
    fn ease(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Step => {
                if value >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::EaseIn => value * value,
            Self::EaseOut => 1.0 - (1.0 - value) * (1.0 - value),
            Self::EaseInOut => {
                if value < 0.5 {
                    2.0 * value * value
                } else {
                    1.0 - (-2.0 * value + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePropertyBinding {
    pub property: String,
    #[serde(default)]
    pub target_node: Option<String>,
    pub target: SceneAnimatedProperty,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
}

impl ScenePropertyBinding {
    fn validate(&self, node_ids: &BTreeSet<String>) -> Result<(), SceneError> {
        validate_required_text("scene property binding property", &self.property)?;
        if let Some(target_node) = &self.target_node
            && !node_ids.contains(target_node)
        {
            return Err(SceneError::invalid(format!(
                "scene property binding {:?} references unknown target node {:?}",
                self.property, target_node
            )));
        }
        for (field, value) in [("scale", self.scale), ("offset", self.offset)] {
            if let Some(value) = value
                && !value.is_finite()
            {
                return Err(SceneError::invalid(format!(
                    "scene property binding {field} must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSystems {
    #[serde(default)]
    pub scenescript: SceneSystemStatus,
    #[serde(default)]
    pub shader_material_graph: SceneSystemStatus,
    #[serde(default)]
    pub particles: SceneSystemStatus,
    #[serde(default)]
    pub parallax: SceneSystemStatus,
    #[serde(default)]
    pub audio_response: SceneSystemStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneSystemStatus {
    Ready,
    Detected,
    #[default]
    Absent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneNativeLowering {
    #[serde(default)]
    pub target_runtime: Option<String>,
    #[serde(default)]
    pub current_runtime: Option<String>,
    #[serde(default)]
    pub fallback: Option<PackagePath>,
    #[serde(default)]
    pub completed_boundaries: Vec<String>,
    #[serde(default)]
    pub pending_boundaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneUnsupportedFeature {
    pub feature: String,
    pub reason: String,
    #[serde(default)]
    pub source_path: Option<String>,
}

impl SceneUnsupportedFeature {
    fn validate(&self) -> Result<(), SceneError> {
        validate_required_text("scene unsupported feature", &self.feature)?;
        validate_required_text("scene unsupported reason", &self.reason)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshot {
    pub time_ms: u64,
    pub layers: Vec<SceneSnapshotLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshotLayer {
    pub id: String,
    pub kind: SceneNodeKind,
    pub source: Option<PackagePath>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
}

fn apply_scene_animated_value(
    transform: &mut SceneTransform,
    opacity: &mut f64,
    property: SceneAnimatedProperty,
    value: f64,
) {
    match property {
        SceneAnimatedProperty::Opacity => *opacity = value.clamp(0.0, 1.0),
        SceneAnimatedProperty::X => transform.x = value,
        SceneAnimatedProperty::Y => transform.y = value,
        SceneAnimatedProperty::ScaleX if value > 0.0 => transform.scale_x = value,
        SceneAnimatedProperty::ScaleY if value > 0.0 => transform.scale_y = value,
        SceneAnimatedProperty::ScaleX | SceneAnimatedProperty::ScaleY => {}
        SceneAnimatedProperty::RotationDeg => transform.rotation_deg = value,
        SceneAnimatedProperty::Custom => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneError {
    message: String,
}

impl SceneError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SceneError {}

fn validate_required_text(field: &str, value: &str) -> Result<(), SceneError> {
    if value.trim().is_empty() {
        Err(SceneError::invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

fn validate_opacity(opacity: f64, owner: &str) -> Result<(), SceneError> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        Err(SceneError::invalid(format!(
            "scene {owner:?} opacity must be finite and between 0 and 1"
        )))
    } else {
        Ok(())
    }
}

const fn default_scene_version() -> u32 {
    SCENE_VERSION
}

const fn default_true() -> bool {
    true
}

const fn default_opacity() -> f64 {
    1.0
}

const fn default_scale() -> f64 {
    1.0
}

const fn default_anchor() -> f64 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_full_scene_document_with_resources_and_native_lowering() {
        let document: SceneDocument = serde_json::from_value(json!({
            "version": 1,
            "source": {
                "format": "wallpaper-engine-scene",
                "metadata": "metadata/source-scene.json",
                "entry": "scene.json"
            },
            "resources": [
                {
                    "id": "resource-background",
                    "type": "image",
                    "source": "assets/scene-resources/background.png",
                    "original_source": "background.png"
                }
            ],
            "nodes": [
                {
                    "id": "node-background",
                    "type": "image",
                    "resource": "resource-background"
                }
            ],
            "native_lowering": {
                "target_runtime": "native-vulkan-full-scene",
                "current_runtime": "native-vulkan-scene-runtime",
                "fallback": "previews/poster.svg"
            }
        }))
        .unwrap();

        document.validate().unwrap();
        assert_eq!(
            document.referenced_paths(),
            vec![
                PackagePath::new("metadata/source-scene.json").unwrap(),
                PackagePath::new("assets/scene-resources/background.png").unwrap(),
                PackagePath::new("previews/poster.svg").unwrap(),
            ]
        );
    }

    #[test]
    fn rejects_nodes_that_reference_unknown_resources() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-background",
                    "type": "image",
                    "resource": "missing-resource"
                }
            ]
        }))
        .unwrap();

        assert!(document.validate().is_err());
    }
}
