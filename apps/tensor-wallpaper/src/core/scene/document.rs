use super::manifest::FitMode;
use super::path::PackagePath;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

mod effects;

use self::effects::{
    push_builtin_effect_snapshot_layers, scene_effect_color_string, scene_effect_value_color,
    scene_effect_adjustment_at,
};

const SCENE_VERSION: u32 = 1;
const SCENE_PARTICLE_DEFAULT_COUNT: u32 = 64;
const SCENE_PARTICLE_MAX_COUNT: u32 = 4096;
const SCENE_PARTICLE_DEFAULT_LIFETIME_MS: u64 = 2_000;
const SCENE_PARTICLE_DEFAULT_SIZE: f64 = 6.0;
const SCENE_PARTICLE_DEFAULT_SPEED: f64 = 24.0;
const SCENE_SAMPLED_IMAGE_DEFAULT_TINT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_default_opacity(value: &f64) -> bool {
    (*value - 1.0).abs() <= f64::EPSILON
}

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
    pub render: SceneRenderSettings,
    #[serde(default)]
    pub camera: SceneCamera,
    #[serde(default)]
    pub import: SceneImportMetadata,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
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
    pub shader_lowering: SceneShaderLowering,
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
        self.render.validate()?;
        self.camera.validate()?;

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
        let mut layers = Vec::new();
        self.snapshot_layers_at_with_property_resolver(time_ms, resolve_property, &mut layers);
        SceneSnapshot { time_ms, layers }
    }

    pub fn snapshot_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
    ) -> SceneSnapshot
    where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        let mut layers = Vec::new();
        self.snapshot_layers_at_with_resolvers(
            time_ms,
            resolve_property,
            resolve_text_property,
            &mut layers,
        );
        SceneSnapshot { time_ms, layers }
    }

    pub fn snapshot_layers_at_with_property_resolver<F>(
        &self,
        time_ms: u64,
        resolve_property: F,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        F: Fn(&str) -> Option<f64>,
    {
        self.snapshot_layers_at_with_resolvers(time_ms, resolve_property, |_| None, layers);
    }

    pub fn snapshot_layers_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        self.snapshot_layers_at_with_resolvers_internal(
            time_ms,
            &resolve_property,
            &resolve_text_property,
            None,
            SceneSnapshotBuildOptions::default(),
            layers,
        );
    }

    pub fn snapshot_compact_layers_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        self.snapshot_layers_at_with_resolvers_internal(
            time_ms,
            &resolve_property,
            &resolve_text_property,
            None,
            SceneSnapshotBuildOptions {
                compact_particle_ids: true,
            },
            layers,
        );
    }

    pub fn snapshot_sampled_image_layers_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        layers: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        let build_index = self.sampled_image_build_index();
        self.snapshot_sampled_image_layers_at_with_resolvers_and_index(
            time_ms,
            resolve_property,
            resolve_text_property,
            &build_index,
            layers,
        );
    }

    pub(crate) fn sampled_image_build_index(&self) -> SceneSnapshotSampledImageBuildIndex {
        SceneSnapshotSampledImageBuildIndex::from_document(self)
    }

    pub(crate) fn snapshot_sampled_image_layers_at_with_resolvers_and_index<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        build_index: &SceneSnapshotSampledImageBuildIndex,
        layers: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        layers.clear();
        let parallax = self.parallax_offset(&resolve_property);
        for node in &self.nodes {
            node.push_sampled_image_snapshot_layers(
                time_ms,
                SceneTransform::default(),
                1.0,
                parallax,
                &self.resources,
                &self.timelines,
                &self.property_bindings,
                build_index,
                &resolve_property,
                &resolve_text_property,
                None,
                None,
                layers,
            );
        }
    }

    pub fn snapshot_solid_layers_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        layers.clear();
        let resources = self
            .resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource))
            .collect::<BTreeMap<_, _>>();
        let parallax = self.parallax_offset(&resolve_property);
        for node in &self.nodes {
            node.push_solid_snapshot_layers(
                time_ms,
                SceneTransform::default(),
                1.0,
                parallax,
                &resources,
                &self.timelines,
                &self.property_bindings,
                &resolve_property,
                &resolve_text_property,
                None,
                None,
                layers,
            );
        }
    }

    pub fn dynamic_solid_geometry_required(&self) -> bool {
        if self
            .nodes
            .iter()
            .any(SceneNode::subtree_has_dynamic_solid_runtime)
        {
            return true;
        }
        if self.property_bindings.iter().any(|binding| {
            binding
                .target_node
                .as_deref()
                .map(|target| {
                    self.node_by_id(target)
                        .is_some_and(SceneNode::subtree_has_solid_visual_geometry)
                })
                .unwrap_or_else(|| {
                    self.nodes
                        .iter()
                        .any(SceneNode::subtree_has_solid_visual_geometry)
                })
        }) {
            return true;
        }
        self.timelines.iter().any(|timeline| {
            timeline
                .target_node
                .as_deref()
                .map(|target| {
                    self.node_by_id(target)
                        .is_some_and(SceneNode::subtree_has_solid_visual_geometry)
                })
                .unwrap_or_else(|| {
                    self.nodes
                        .iter()
                        .any(SceneNode::subtree_has_solid_visual_geometry)
                })
        })
    }

    fn node_by_id(&self, id: &str) -> Option<&SceneNode> {
        self.nodes.iter().find_map(|node| node.find_by_id(id))
    }

    pub fn snapshot_visible_layers_at_with_resolvers<N, T>(
        &self,
        time_ms: u64,
        resolve_property: N,
        resolve_text_property: T,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        self.snapshot_layers_at_with_resolvers_internal(
            time_ms,
            &resolve_property,
            &resolve_text_property,
            SceneSnapshotVisibility::from_size(self.size),
            SceneSnapshotBuildOptions::default(),
            layers,
        );
    }

    fn snapshot_layers_at_with_resolvers_internal<N, T>(
        &self,
        time_ms: u64,
        resolve_property: &N,
        resolve_text_property: &T,
        visibility: Option<SceneSnapshotVisibility>,
        options: SceneSnapshotBuildOptions,
        layers: &mut Vec<SceneSnapshotLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
        T: Fn(&str) -> Option<String>,
    {
        layers.clear();
        let resources = self
            .resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource))
            .collect::<BTreeMap<_, _>>();
        if let Some(clear_layer) = self.render_clear_layer() {
            layers.push(clear_layer);
        }
        let parallax = self.parallax_offset(resolve_property);
        for node in &self.nodes {
            node.push_snapshot_layers(
                time_ms,
                SceneTransform::default(),
                1.0,
                parallax,
                &resources,
                &self.timelines,
                &self.property_bindings,
                resolve_property,
                resolve_text_property,
                visibility,
                None,
                options,
                layers,
            );
        }
    }

    fn parallax_offset(
        &self,
        resolve_property: &impl Fn(&str) -> Option<f64>,
    ) -> SceneParallaxOffset {
        let amount = self
            .render
            .parallax
            .as_ref()
            .and_then(|parallax| parallax.amount)
            .unwrap_or(0.0);
        if amount == 0.0 {
            return SceneParallaxOffset::default();
        }
        let x = resolve_scene_property(
            resolve_property,
            &["scene.parallax.x", "scene_parallax_x", "parallax_x"],
        )
        .unwrap_or(0.0);
        let y = resolve_scene_property(
            resolve_property,
            &["scene.parallax.y", "scene_parallax_y", "parallax_y"],
        )
        .unwrap_or(0.0);
        SceneParallaxOffset {
            x: x * amount,
            y: y * amount,
        }
    }

    fn render_clear_layer(&self) -> Option<SceneSnapshotLayer> {
        if self.render.clear_enabled == Some(false) {
            return None;
        }
        let color = self.render.clear_color.as_ref()?.trim();
        if color.is_empty() {
            return None;
        }
        Some(SceneSnapshotLayer {
            id: "scene-render-clear-color".to_owned(),
            kind: SceneNodeKind::Color,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: SceneAlphaTextureMode::Multiply,
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: SceneEffectMotion::default(),
            blend_mode: SceneBlendMode::Alpha,
            audio: Vec::new(),
            color: Some(color.to_owned()),
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: self.size.map(|size| f64::from(size.width)),
            height: self.size.map(|size| f64::from(size.height)),
            mesh: None,
            parallax_depth: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneProfile {
    #[default]
    RenderingDeviceFullScene,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderSettings {
    #[serde(default)]
    pub clear_color: Option<String>,
    #[serde(default)]
    pub clear_enabled: Option<bool>,
    #[serde(default)]
    pub ambient_color: Option<String>,
    #[serde(default)]
    pub hdr: Option<bool>,
    #[serde(default)]
    pub bloom: Option<SceneBloomSettings>,
    #[serde(default)]
    pub parallax: Option<SceneParallaxSettings>,
    #[serde(default)]
    pub environment: BTreeMap<String, Value>,
}

impl SceneRenderSettings {
    fn validate(&self) -> Result<(), SceneError> {
        if let Some(bloom) = &self.bloom {
            bloom.validate()?;
        }
        if let Some(parallax) = &self.parallax {
            parallax.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneBloomSettings {
    #[serde(default)]
    pub strength: Option<f64>,
    #[serde(default)]
    pub threshold: Option<f64>,
    #[serde(default)]
    pub hdr_strength: Option<f64>,
    #[serde(default)]
    pub hdr_threshold: Option<f64>,
    #[serde(default)]
    pub tint: Option<String>,
}

impl SceneBloomSettings {
    fn validate(&self) -> Result<(), SceneError> {
        validate_optional_finite("scene bloom strength", self.strength)?;
        validate_optional_finite("scene bloom threshold", self.threshold)?;
        validate_optional_finite("scene bloom hdr_strength", self.hdr_strength)?;
        validate_optional_finite("scene bloom hdr_threshold", self.hdr_threshold)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneParallaxSettings {
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub delay: Option<f64>,
    #[serde(default)]
    pub mouse_influence: Option<Value>,
}

impl SceneParallaxSettings {
    fn validate(&self) -> Result<(), SceneError> {
        validate_optional_finite("scene parallax amount", self.amount)?;
        validate_optional_finite("scene parallax delay", self.delay)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneCamera {
    #[serde(default)]
    pub center: Option<SceneVector3>,
    #[serde(default)]
    pub eye: Option<SceneVector3>,
    #[serde(default)]
    pub up: Option<SceneVector3>,
    #[serde(default)]
    pub near_z: Option<f64>,
    #[serde(default)]
    pub far_z: Option<f64>,
    #[serde(default)]
    pub fov: Option<f64>,
    #[serde(default)]
    pub zoom: Option<f64>,
}

impl SceneCamera {
    fn validate(&self) -> Result<(), SceneError> {
        for (field, value) in [
            ("near_z", self.near_z),
            ("far_z", self.far_z),
            ("fov", self.fov),
            ("zoom", self.zoom),
        ] {
            validate_optional_finite(&format!("scene camera {field}"), value)?;
        }
        for (field, value) in [("center", self.center), ("eye", self.eye), ("up", self.up)] {
            if let Some(value) = value {
                value.validate(&format!("scene camera {field}"))?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneVector3 {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

impl SceneVector3 {
    fn validate(self, owner: &str) -> Result<(), SceneError> {
        for (field, value) in [("x", self.x), ("y", self.y), ("z", self.z)] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "{owner} {field} must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneImportMetadata {
    #[serde(default)]
    pub source_format: Option<String>,
    #[serde(default)]
    pub source_version: Option<i64>,
    #[serde(default)]
    pub object_count: usize,
    #[serde(default)]
    pub feature_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneResource {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: SceneResourceKind,
    pub source: PackagePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
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
    Texture,
    Model,
    Material,
    Effect,
    Particle,
    Font,
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
    pub provenance: Option<SceneNodeProvenance>,
    #[serde(default)]
    pub effects: Vec<SceneEffect>,
    #[serde(default)]
    pub audio: Vec<SceneAudioCue>,
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
    pub mesh: Option<Arc<SceneMesh>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_animation_layers: Vec<ScenePuppetAnimationLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub puppet_attachment: Option<String>,
    #[serde(default)]
    pub parallax_depth: Option<f64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_resource: Option<String>,
    #[serde(default)]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub text_align: Option<SceneTextAlign>,
    #[serde(default)]
    #[serde(rename = "path")]
    pub path_data: Option<String>,
    #[serde(default)]
    pub path_fill_rule: ScenePathFillRule,
    #[serde(default)]
    pub fit: FitMode,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub children: Vec<SceneNode>,
}
