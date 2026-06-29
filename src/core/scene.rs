use super::manifest::FitMode;
use super::path::PackagePath;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const SCENE_VERSION: u32 = 1;
const SCENE_PARTICLE_DEFAULT_COUNT: u32 = 64;
const SCENE_PARTICLE_MAX_COUNT: u32 = 4096;
const SCENE_PARTICLE_DEFAULT_LIFETIME_MS: u64 = 2_000;
const SCENE_PARTICLE_DEFAULT_SIZE: f64 = 6.0;
const SCENE_PARTICLE_DEFAULT_SPEED: f64 = 24.0;

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

    pub fn snapshot_sampled_image_layers_at_with_resolvers<N>(
        &self,
        time_ms: u64,
        resolve_property: N,
        layers: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) where
        N: Fn(&str) -> Option<f64>,
    {
        layers.clear();
        let resources = self
            .resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource))
            .collect::<BTreeMap<_, _>>();
        let parallax = self.parallax_offset(&resolve_property);
        for node in &self.nodes {
            node.push_sampled_image_snapshot_layers(
                time_ms,
                SceneTransform::default(),
                1.0,
                parallax,
                &resources,
                &self.timelines,
                &self.property_bindings,
                &resolve_property,
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
            texture_region: None,
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
    pub mesh: Option<SceneMesh>,
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
        if let Some(mesh) = &self.mesh {
            mesh.validate(&self.id)?;
        }
        if let Some(resource) = &self.resource
            && !resource_ids.contains(resource)
        {
            return Err(SceneError::invalid(format!(
                "scene node {:?} references unknown resource {:?}",
                self.id, resource
            )));
        }
        if let Some(font_resource) = &self.font_resource
            && !resource_ids.contains(font_resource)
        {
            return Err(SceneError::invalid(format!(
                "scene node {:?} references unknown font resource {:?}",
                self.id, font_resource
            )));
        }
        if let Some(provenance) = &self.provenance {
            provenance.validate(&self.id)?;
        }
        validate_optional_finite("scene node parallax_depth", self.parallax_depth)?;
        for effect in &self.effects {
            effect.validate(&self.id)?;
        }
        for audio in &self.audio {
            audio.validate(&self.id)?;
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
        parallax: SceneParallaxOffset,
        resources: &BTreeMap<&str, &SceneResource>,
        timelines: &[SceneTimeline],
        property_bindings: &[ScenePropertyBinding],
        resolve_property: &impl Fn(&str) -> Option<f64>,
        resolve_text_property: &impl Fn(&str) -> Option<String>,
        visibility: Option<SceneSnapshotVisibility>,
        options: SceneSnapshotBuildOptions,
        output: &mut Vec<SceneSnapshotLayer>,
    ) {
        if !self.visible {
            return;
        }
        let mut transform = self.transform;
        let mut opacity = self.opacity;
        let mut width = self.width;
        let mut height = self.height;
        let mut corner_radius = self.corner_radius;
        for timeline in timelines
            .iter()
            .filter(|timeline| timeline.target_node.as_deref() == Some(self.id.as_str()))
        {
            for channel in &timeline.channels {
                let value = channel.value_at(time_ms);
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    channel.property,
                    value,
                );
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
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    binding.target,
                    value,
                );
            }
        }

        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        if self.kind == SceneNodeKind::ParticleEmitter
            && self.push_particle_snapshot_layers(
                time_ms, transform, opacity, resources, visibility, options, output,
            )
        {
            for child in &self.children {
                child.push_snapshot_layers(
                    time_ms,
                    transform,
                    opacity,
                    parallax,
                    resources,
                    timelines,
                    property_bindings,
                    resolve_property,
                    resolve_text_property,
                    visibility,
                    options,
                    output,
                );
            }
            return;
        }

        if self.kind != SceneNodeKind::Group {
            let texture_region = scene_texture_region_from_properties(&self.properties, time_ms);
            let text = scene_text_from_properties(&self.properties, resolve_text_property)
                .or_else(|| self.text.clone());
            let audio = scene_audio_cues_for_snapshot(&self.audio, resolve_property);
            let layer = SceneSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: self
                    .resource
                    .as_deref()
                    .and_then(|resource| resources.get(resource))
                    .map(|resource| resource.source.clone()),
                texture_region,
                audio,
                color: self.color.clone(),
                stroke_color: self.stroke_color.clone(),
                stroke_width: self.stroke_width,
                corner_radius,
                width,
                height,
                mesh: self.mesh.clone(),
                parallax_depth: self.parallax_depth,
                text,
                font_size: self.font_size,
                font_family: self.font_family.clone(),
                font_source: self
                    .font_resource
                    .as_deref()
                    .and_then(|resource| resources.get(resource))
                    .map(|resource| resource.source.clone()),
                font_weight: self.font_weight.clone(),
                text_align: self.text_align,
                path_data: self.path_data.clone(),
                path_fill_rule: self.path_fill_rule,
                fit: self.fit,
                opacity,
                transform,
            };
            if scene_snapshot_layer_intersects_visibility(&layer, visibility) {
                push_native_effect_snapshot_layers(&self.effects, &layer, output);
                output.push(layer);
            }
        }
        for child in &self.children {
            child.push_snapshot_layers(
                time_ms,
                transform,
                opacity,
                parallax,
                resources,
                timelines,
                property_bindings,
                resolve_property,
                resolve_text_property,
                visibility,
                options,
                output,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_sampled_image_snapshot_layers(
        &self,
        time_ms: u64,
        parent_transform: SceneTransform,
        parent_opacity: f64,
        parallax: SceneParallaxOffset,
        resources: &BTreeMap<&str, &SceneResource>,
        timelines: &[SceneTimeline],
        property_bindings: &[ScenePropertyBinding],
        resolve_property: &impl Fn(&str) -> Option<f64>,
        visibility: Option<SceneSnapshotVisibility>,
        output: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) {
        if !self.visible {
            return;
        }
        let mut transform = self.transform;
        let mut opacity = self.opacity;
        let mut width = self.width;
        let mut height = self.height;
        let mut corner_radius = self.corner_radius;
        for timeline in timelines
            .iter()
            .filter(|timeline| timeline.target_node.as_deref() == Some(self.id.as_str()))
        {
            for channel in &timeline.channels {
                let value = channel.value_at(time_ms);
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    channel.property,
                    value,
                );
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
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    binding.target,
                    value,
                );
            }
        }

        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        if self.kind == SceneNodeKind::ParticleEmitter
            && self.push_particle_sampled_image_snapshot_layers(
                time_ms, transform, opacity, resources, visibility, output,
            )
        {
            for child in &self.children {
                child.push_sampled_image_snapshot_layers(
                    time_ms,
                    transform,
                    opacity,
                    parallax,
                    resources,
                    timelines,
                    property_bindings,
                    resolve_property,
                    visibility,
                    output,
                );
            }
            return;
        }

        if self.kind == SceneNodeKind::Image {
            let source = self
                .resource
                .as_deref()
                .and_then(|resource| resources.get(resource));
            let layer = SceneSnapshotSampledImageLayer {
                has_source: source.is_some(),
                texture_region: scene_texture_region_from_properties(&self.properties, time_ms),
                width,
                height,
                mesh: self.mesh.clone(),
                fit: self.fit,
                opacity,
                transform,
            };
            if scene_sampled_image_snapshot_layer_intersects_visibility(&layer, visibility) {
                output.push(layer);
            }
        }
        for child in &self.children {
            child.push_sampled_image_snapshot_layers(
                time_ms,
                transform,
                opacity,
                parallax,
                resources,
                timelines,
                property_bindings,
                resolve_property,
                visibility,
                output,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_solid_snapshot_layers(
        &self,
        time_ms: u64,
        parent_transform: SceneTransform,
        parent_opacity: f64,
        parallax: SceneParallaxOffset,
        resources: &BTreeMap<&str, &SceneResource>,
        timelines: &[SceneTimeline],
        property_bindings: &[ScenePropertyBinding],
        resolve_property: &impl Fn(&str) -> Option<f64>,
        resolve_text_property: &impl Fn(&str) -> Option<String>,
        visibility: Option<SceneSnapshotVisibility>,
        output: &mut Vec<SceneSnapshotLayer>,
    ) {
        if !self.visible {
            return;
        }
        let mut transform = self.transform;
        let mut opacity = self.opacity;
        let mut width = self.width;
        let mut height = self.height;
        let mut corner_radius = self.corner_radius;
        for timeline in timelines
            .iter()
            .filter(|timeline| timeline.target_node.as_deref() == Some(self.id.as_str()))
        {
            for channel in &timeline.channels {
                let value = channel.value_at(time_ms);
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    channel.property,
                    value,
                );
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
                apply_scene_animated_value(
                    &mut transform,
                    &mut opacity,
                    &mut width,
                    &mut height,
                    &mut corner_radius,
                    binding.target,
                    value,
                );
            }
        }

        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        if self.kind == SceneNodeKind::ParticleEmitter
            && self.push_particle_solid_snapshot_layers(
                time_ms, transform, opacity, resources, visibility, output,
            )
        {
            for child in &self.children {
                child.push_solid_snapshot_layers(
                    time_ms,
                    transform,
                    opacity,
                    parallax,
                    resources,
                    timelines,
                    property_bindings,
                    resolve_property,
                    resolve_text_property,
                    visibility,
                    output,
                );
            }
            return;
        }

        if self.subtree_self_has_solid_visual_geometry() {
            let text = scene_text_from_properties(&self.properties, resolve_text_property)
                .or_else(|| self.text.clone());
            let layer = SceneSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: None,
                texture_region: None,
                audio: Vec::new(),
                color: self.color.clone(),
                stroke_color: self.stroke_color.clone(),
                stroke_width: self.stroke_width,
                corner_radius,
                width,
                height,
                mesh: self.mesh.clone(),
                parallax_depth: self.parallax_depth,
                text,
                font_size: self.font_size,
                font_family: self.font_family.clone(),
                font_source: self
                    .font_resource
                    .as_deref()
                    .and_then(|resource| resources.get(resource))
                    .map(|resource| resource.source.clone()),
                font_weight: self.font_weight.clone(),
                text_align: self.text_align,
                path_data: self.path_data.clone(),
                path_fill_rule: self.path_fill_rule,
                fit: self.fit,
                opacity,
                transform,
            };
            if scene_snapshot_layer_intersects_visibility(&layer, visibility) {
                push_native_effect_snapshot_layers(&self.effects, &layer, output);
                output.push(layer);
            }
        }
        for child in &self.children {
            child.push_solid_snapshot_layers(
                time_ms,
                transform,
                opacity,
                parallax,
                resources,
                timelines,
                property_bindings,
                resolve_property,
                resolve_text_property,
                visibility,
                output,
            );
        }
    }

    fn push_particle_snapshot_layers(
        &self,
        time_ms: u64,
        transform: SceneTransform,
        opacity: f64,
        resources: &BTreeMap<&str, &SceneResource>,
        visibility: Option<SceneSnapshotVisibility>,
        options: SceneSnapshotBuildOptions,
        output: &mut Vec<SceneSnapshotLayer>,
    ) -> bool {
        let Some(settings) = SceneParticleEmitterSettings::from_node(self) else {
            return false;
        };
        let particle_count = settings.count.min(SCENE_PARTICLE_MAX_COUNT);
        if particle_count == 0 || opacity <= 0.0 {
            return true;
        }
        let source = self
            .resource
            .as_deref()
            .and_then(|resource| resources.get(resource))
            .map(|resource| resource.source.clone());
        let texture_region = scene_texture_region_from_properties(&self.properties, time_ms);
        let layer_kind = if source.is_some() {
            SceneNodeKind::Image
        } else {
            settings.shape
        };
        output.reserve(particle_count as usize);
        let (parent_sin, parent_cos) = transform.rotation_deg.to_radians().sin_cos();
        let particle_id_prefix =
            (!options.compact_particle_ids).then(|| format!("{}::particle-", self.id));
        for index in 0..particle_count {
            let Some((particle_opacity, x, y, rotation_deg)) =
                settings.opacity_and_transform_at(time_ms, index)
            else {
                continue;
            };
            let layer_opacity = opacity * particle_opacity;
            if layer_opacity <= 0.0 {
                continue;
            }
            let particle_transform = scene_compose_particle_transform(
                transform,
                parent_sin,
                parent_cos,
                x,
                y,
                rotation_deg,
            );
            if !scene_snapshot_visual_bounds_intersects(
                Some(settings.particle_width),
                Some(settings.particle_height),
                None,
                particle_transform,
                visibility,
            ) {
                continue;
            }
            let id = if let Some(prefix) = particle_id_prefix.as_deref() {
                let mut id = String::with_capacity(prefix.len() + 10);
                id.push_str(prefix);
                {
                    use std::fmt::Write as _;
                    let _ = write!(&mut id, "{index}");
                }
                id
            } else {
                String::new()
            };
            output.push(SceneSnapshotLayer {
                id,
                kind: layer_kind,
                source: source.clone(),
                texture_region,
                audio: if index == 0 {
                    self.audio.clone()
                } else {
                    Vec::new()
                },
                color: Some(settings.color.clone()),
                stroke_color: None,
                stroke_width: None,
                corner_radius: None,
                width: Some(settings.particle_width),
                height: Some(settings.particle_height),
                mesh: None,
                parallax_depth: self.parallax_depth,
                text: None,
                font_size: None,
                font_family: None,
                font_source: None,
                font_weight: None,
                text_align: None,
                path_data: None,
                path_fill_rule: ScenePathFillRule::default(),
                fit: self.fit,
                opacity: layer_opacity.clamp(0.0, 1.0),
                transform: particle_transform,
            });
        }
        true
    }

    fn push_particle_sampled_image_snapshot_layers(
        &self,
        time_ms: u64,
        transform: SceneTransform,
        opacity: f64,
        resources: &BTreeMap<&str, &SceneResource>,
        visibility: Option<SceneSnapshotVisibility>,
        output: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) -> bool {
        let Some(settings) = SceneParticleEmitterSettings::from_node(self) else {
            return false;
        };
        let particle_count = settings.count.min(SCENE_PARTICLE_MAX_COUNT);
        if particle_count == 0 || opacity <= 0.0 {
            return true;
        }
        let has_source = self
            .resource
            .as_deref()
            .and_then(|resource| resources.get(resource))
            .is_some();
        if !has_source {
            return true;
        }
        let texture_region = scene_texture_region_from_properties(&self.properties, time_ms);
        output.reserve(particle_count as usize);
        let (parent_sin, parent_cos) = transform.rotation_deg.to_radians().sin_cos();
        for index in 0..particle_count {
            let Some((particle_opacity, x, y, rotation_deg)) =
                settings.opacity_and_transform_at(time_ms, index)
            else {
                continue;
            };
            let layer_opacity = opacity * particle_opacity;
            if layer_opacity <= 0.0 {
                continue;
            }
            let particle_transform = scene_compose_particle_transform(
                transform,
                parent_sin,
                parent_cos,
                x,
                y,
                rotation_deg,
            );
            if !scene_snapshot_visual_bounds_intersects(
                Some(settings.particle_width),
                Some(settings.particle_height),
                None,
                particle_transform,
                visibility,
            ) {
                continue;
            }
            output.push(SceneSnapshotSampledImageLayer {
                has_source: true,
                texture_region,
                width: Some(settings.particle_width),
                height: Some(settings.particle_height),
                mesh: None,
                fit: self.fit,
                opacity: layer_opacity.clamp(0.0, 1.0),
                transform: particle_transform,
            });
        }
        true
    }

    fn push_particle_solid_snapshot_layers(
        &self,
        time_ms: u64,
        transform: SceneTransform,
        opacity: f64,
        resources: &BTreeMap<&str, &SceneResource>,
        visibility: Option<SceneSnapshotVisibility>,
        output: &mut Vec<SceneSnapshotLayer>,
    ) -> bool {
        let Some(settings) = SceneParticleEmitterSettings::from_node(self) else {
            return false;
        };
        let particle_count = settings.count.min(SCENE_PARTICLE_MAX_COUNT);
        if particle_count == 0 || opacity <= 0.0 {
            return true;
        }
        let has_source = self
            .resource
            .as_deref()
            .and_then(|resource| resources.get(resource))
            .is_some();
        if has_source {
            return true;
        }
        output.reserve(particle_count as usize);
        let (parent_sin, parent_cos) = transform.rotation_deg.to_radians().sin_cos();
        for index in 0..particle_count {
            let Some((particle_opacity, x, y, rotation_deg)) =
                settings.opacity_and_transform_at(time_ms, index)
            else {
                continue;
            };
            let layer_opacity = opacity * particle_opacity;
            if layer_opacity <= 0.0 {
                continue;
            }
            let particle_transform = scene_compose_particle_transform(
                transform,
                parent_sin,
                parent_cos,
                x,
                y,
                rotation_deg,
            );
            if !scene_snapshot_visual_bounds_intersects(
                Some(settings.particle_width),
                Some(settings.particle_height),
                None,
                particle_transform,
                visibility,
            ) {
                continue;
            }
            output.push(SceneSnapshotLayer {
                id: String::new(),
                kind: settings.shape,
                source: None,
                texture_region: None,
                audio: Vec::new(),
                color: Some(settings.color.clone()),
                stroke_color: None,
                stroke_width: None,
                corner_radius: None,
                width: Some(settings.particle_width),
                height: Some(settings.particle_height),
                mesh: None,
                parallax_depth: self.parallax_depth,
                text: None,
                font_size: None,
                font_family: None,
                font_source: None,
                font_weight: None,
                text_align: None,
                path_data: None,
                path_fill_rule: ScenePathFillRule::default(),
                fit: self.fit,
                opacity: layer_opacity.clamp(0.0, 1.0),
                transform: particle_transform,
            });
        }
        true
    }

    fn find_by_id(&self, id: &str) -> Option<&SceneNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find_by_id(id))
    }

    fn subtree_has_dynamic_solid_runtime(&self) -> bool {
        self.particle_emitter_outputs_solid()
            || self
                .children
                .iter()
                .any(SceneNode::subtree_has_dynamic_solid_runtime)
    }

    fn subtree_has_solid_visual_geometry(&self) -> bool {
        let self_has_solid = match self.kind {
            SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::AudioResponse => true,
            SceneNodeKind::ParticleEmitter => self.particle_emitter_outputs_solid(),
            SceneNodeKind::Group
            | SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Audio
            | SceneNodeKind::Shader
            | SceneNodeKind::Script
            | SceneNodeKind::Unknown => false,
        };
        self_has_solid
            || self
                .children
                .iter()
                .any(SceneNode::subtree_has_solid_visual_geometry)
    }

    fn subtree_self_has_solid_visual_geometry(&self) -> bool {
        match self.kind {
            SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::AudioResponse => true,
            SceneNodeKind::ParticleEmitter => self.particle_emitter_outputs_solid(),
            SceneNodeKind::Group
            | SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Audio
            | SceneNodeKind::Shader
            | SceneNodeKind::Script
            | SceneNodeKind::Unknown => false,
        }
    }

    fn particle_emitter_outputs_solid(&self) -> bool {
        self.kind == SceneNodeKind::ParticleEmitter
            && self.resource.is_none()
            && SceneParticleEmitterSettings::from_node(self)
                .is_some_and(|settings| settings.count > 0)
    }
}

fn push_native_effect_snapshot_layers(
    effects: &[SceneEffect],
    base: &SceneSnapshotLayer,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    if base.kind != SceneNodeKind::Text || base.text.as_deref().is_none_or(str::is_empty) {
        return;
    }
    for (effect_index, effect) in effects.iter().enumerate() {
        if effect.runtime.as_deref() != Some("native-text-glow") {
            continue;
        }
        push_native_text_glow_snapshot_layers(effect_index, effect, base, output);
    }
}

fn push_native_text_glow_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let radius = scene_effect_property_f64(effect, "radius", 2.0).clamp(0.25, 16.0);
    let opacity =
        (base.opacity * scene_effect_property_f64(effect, "opacity", 0.12)).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let samples = effect
        .properties
        .get("samples")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(8)
        .clamp(4, NATIVE_TEXT_GLOW_OFFSETS.len());
    for (sample_index, [x, y]) in NATIVE_TEXT_GLOW_OFFSETS.iter().take(samples).enumerate() {
        let mut layer = base.clone();
        layer.id = format!(
            "{}::native-text-glow-{}-{}",
            base.id, effect_index, sample_index
        );
        layer.opacity = opacity;
        layer.transform.x += x * radius;
        layer.transform.y += y * radius;
        output.push(layer);
    }
}

const NATIVE_TEXT_GLOW_OFFSETS: [[f64; 2]; 8] = [
    [-1.0, 0.0],
    [1.0, 0.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [-0.70710678118, -0.70710678118],
    [0.70710678118, -0.70710678118],
    [-0.70710678118, 0.70710678118],
    [0.70710678118, 0.70710678118],
];

fn scene_effect_property_f64(effect: &SceneEffect, key: &str, default: f64) -> f64 {
    effect
        .properties
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

#[derive(Debug, Clone, PartialEq)]
struct SceneParticleEmitterSettings {
    count: u32,
    seed: u64,
    lifetime_ms: u64,
    loop_playback: bool,
    spawn_width: f64,
    spawn_height: f64,
    particle_width: f64,
    particle_height: f64,
    speed_min: f64,
    speed_max: f64,
    direction_deg: f64,
    spread_deg: f64,
    gravity_x: f64,
    gravity_y: f64,
    fade: bool,
    color: String,
    shape: SceneNodeKind,
}

impl SceneParticleEmitterSettings {
    fn from_node(node: &SceneNode) -> Option<Self> {
        let particle = node.properties.get("particle").and_then(Value::as_object);
        let count = scene_particle_u32(particle, "count")
            .or_else(|| scene_particle_u32(particle, "max_count"))
            .unwrap_or_else(|| {
                let lifetime_seconds = scene_particle_f64(particle, "lifetime")
                    .or_else(|| scene_particle_f64(particle, "lifetime_seconds"))
                    .unwrap_or(SCENE_PARTICLE_DEFAULT_LIFETIME_MS as f64 / 1000.0);
                scene_particle_f64(particle, "rate")
                    .filter(|rate| rate.is_finite() && *rate > 0.0)
                    .map(|rate| (rate * lifetime_seconds).round().max(1.0) as u32)
                    .unwrap_or(SCENE_PARTICLE_DEFAULT_COUNT)
            })
            .clamp(0, SCENE_PARTICLE_MAX_COUNT);
        let lifetime_ms = scene_particle_u64(particle, "lifetime_ms")
            .or_else(|| {
                scene_particle_f64(particle, "lifetime")
                    .or_else(|| scene_particle_f64(particle, "lifetime_seconds"))
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .map(|value| (value * 1000.0).round() as u64)
            })
            .unwrap_or(SCENE_PARTICLE_DEFAULT_LIFETIME_MS)
            .max(1);
        let particle_width = scene_particle_f64(particle, "width")
            .or_else(|| scene_particle_f64(particle, "size"))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(SCENE_PARTICLE_DEFAULT_SIZE);
        let particle_height = scene_particle_f64(particle, "height")
            .or_else(|| scene_particle_f64(particle, "size"))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(particle_width);
        let speed = scene_particle_f64(particle, "speed")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(SCENE_PARTICLE_DEFAULT_SPEED);
        let speed_min = scene_particle_f64(particle, "speed_min")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(speed);
        let speed_max = scene_particle_f64(particle, "speed_max")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(speed)
            .max(speed_min);
        let spawn_width = scene_particle_f64(particle, "spawn_width")
            .or_else(|| scene_particle_f64(particle, "emitter_width"))
            .or(node.width)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let spawn_height = scene_particle_f64(particle, "spawn_height")
            .or_else(|| scene_particle_f64(particle, "emitter_height"))
            .or(node.height)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let shape = match scene_particle_string(particle, "shape")
            .unwrap_or_else(|| "rectangle".to_owned())
            .to_ascii_lowercase()
            .as_str()
        {
            "ellipse" | "circle" => SceneNodeKind::Ellipse,
            _ => SceneNodeKind::Rectangle,
        };
        Some(Self {
            count,
            seed: scene_particle_u64(particle, "seed")
                .unwrap_or_else(|| scene_particle_seed_from_id(&node.id)),
            lifetime_ms,
            loop_playback: scene_particle_bool(particle, "loop").unwrap_or(true),
            spawn_width,
            spawn_height,
            particle_width,
            particle_height,
            speed_min,
            speed_max,
            direction_deg: scene_particle_f64(particle, "direction_deg").unwrap_or(-90.0),
            spread_deg: scene_particle_f64(particle, "spread_deg").unwrap_or(360.0),
            gravity_x: scene_particle_f64(particle, "gravity_x").unwrap_or(0.0),
            gravity_y: scene_particle_f64(particle, "gravity_y").unwrap_or(0.0),
            fade: scene_particle_bool(particle, "fade").unwrap_or(true),
            color: scene_particle_string(particle, "color")
                .or_else(|| node.color.clone())
                .unwrap_or_else(|| "#ffffff".to_owned()),
            shape,
        })
    }

    fn age_seconds(&self, time_ms: u64, index: u32) -> Option<f64> {
        let phase = scene_particle_unit(self.seed, index, 0);
        let phase_ms = (phase * self.lifetime_ms as f64).round() as u64;
        let local_ms = if self.loop_playback {
            time_ms.wrapping_add(phase_ms) % self.lifetime_ms
        } else {
            let started_at = phase_ms.min(self.lifetime_ms);
            if time_ms < started_at {
                return None;
            }
            (time_ms - started_at).min(self.lifetime_ms)
        };
        Some(local_ms as f64 / 1000.0)
    }

    #[inline]
    fn opacity_and_transform_at(&self, time_ms: u64, index: u32) -> Option<(f64, f64, f64, f64)> {
        let age = self.age_seconds(time_ms, index)?;
        let progress = (age * 1000.0 / self.lifetime_ms as f64).clamp(0.0, 1.0);
        let opacity = if self.fade { 1.0 - progress } else { 1.0 };
        let spawn_x = (scene_particle_unit(self.seed, index, 1) - 0.5) * self.spawn_width;
        let spawn_y = (scene_particle_unit(self.seed, index, 2) - 0.5) * self.spawn_height;
        let speed = self.speed_min
            + (self.speed_max - self.speed_min) * scene_particle_unit(self.seed, index, 3);
        let direction =
            self.direction_deg + (scene_particle_unit(self.seed, index, 4) - 0.5) * self.spread_deg;
        let radians = direction.to_radians();
        let (direction_sin, direction_cos) = radians.sin_cos();
        let x = spawn_x + direction_cos * speed * age + 0.5 * self.gravity_x * age * age;
        let y = spawn_y + direction_sin * speed * age + 0.5 * self.gravity_y * age * age;
        Some((opacity, x, y, direction))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneNodeProvenance {
    #[serde(default)]
    pub source_format: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub attachment: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub original_type: Option<String>,
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub transform: Option<SceneSourceTransform>,
    #[serde(default)]
    pub model: Option<SceneSourceModel>,
    #[serde(default)]
    pub particle: Option<Value>,
    #[serde(default)]
    pub animation_layers: Vec<Value>,
    #[serde(default)]
    pub instance: Option<Value>,
    #[serde(default)]
    pub instance_override: Option<Value>,
}

impl SceneNodeProvenance {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("source_format", self.source_format.as_deref()),
            ("source_id", self.source_id.as_deref()),
            ("parent_id", self.parent_id.as_deref()),
            ("attachment", self.attachment.as_deref()),
            ("original_type", self.original_type.as_deref()),
            ("original_path", self.original_path.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} provenance {field} must not be empty"
                )));
            }
        }
        for dependency in &self.dependencies {
            validate_required_text(
                &format!("scene node {node_id:?} provenance dependency"),
                dependency,
            )?;
        }
        if let Some(transform) = &self.transform {
            transform.validate(node_id)?;
        }
        if let Some(model) = &self.model {
            model.validate(node_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneSourceTransform {
    #[serde(default)]
    pub origin: Option<SceneVector3>,
    #[serde(default)]
    pub angles: Option<SceneVector3>,
    #[serde(default)]
    pub scale: Option<SceneVector3>,
    #[serde(default)]
    pub pivot: Option<SceneVector3>,
    #[serde(default)]
    pub size: Option<SceneVector3>,
    #[serde(default)]
    pub alignment: Option<String>,
}

impl SceneSourceTransform {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("origin", self.origin),
            ("angles", self.angles),
            ("scale", self.scale),
            ("pivot", self.pivot),
            ("size", self.size),
        ] {
            if let Some(value) = value {
                value.validate(&format!("scene node {node_id:?} source transform {field}"))?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSourceModel {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub utility: Option<String>,
    #[serde(default)]
    pub builtin: Option<bool>,
    #[serde(default)]
    pub model_resource: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub material_resource: Option<String>,
    #[serde(default)]
    pub puppet: Option<String>,
    #[serde(default)]
    pub solid_layer: Option<bool>,
    #[serde(default)]
    pub passthrough: Option<bool>,
    #[serde(default)]
    pub textures: Vec<String>,
    #[serde(default)]
    pub texture_resources: Vec<String>,
}

impl SceneSourceModel {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("source", self.source.as_deref()),
            ("utility", self.utility.as_deref()),
            ("model_resource", self.model_resource.as_deref()),
            ("material", self.material.as_deref()),
            ("material_resource", self.material_resource.as_deref()),
            ("puppet", self.puppet.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} source model {field} must not be empty"
                )));
            }
        }
        for texture in &self.textures {
            validate_required_text(
                &format!("scene node {node_id:?} source model texture"),
                texture,
            )?;
        }
        for texture_resource in &self.texture_resources {
            validate_required_text(
                &format!("scene node {node_id:?} source model texture resource"),
                texture_resource,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffect {
    pub file: String,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub visible: Option<Value>,
    #[serde(default)]
    pub passes: Vec<SceneEffectPass>,
}

impl SceneEffect {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        validate_required_text(&format!("scene node {node_id:?} effect file"), &self.file)?;
        if let Some(resource) = &self.resource {
            validate_required_text(&format!("scene node {node_id:?} effect resource"), resource)?;
        }
        if let Some(runtime) = &self.runtime {
            validate_required_text(&format!("scene node {node_id:?} effect runtime"), runtime)?;
        }
        for pass in &self.passes {
            pass.validate(node_id, &self.file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectPass {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub textures: Vec<Option<String>>,
    #[serde(default)]
    pub combos: BTreeMap<String, i64>,
    #[serde(default)]
    pub constant_shader_values: BTreeMap<String, Value>,
    #[serde(default)]
    pub user_textures: Option<Value>,
}

impl SceneEffectPass {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        for texture in self.textures.iter().flatten() {
            if texture.trim().is_empty() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} effect {effect_file:?} texture reference must not be empty"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneAudioCue {
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub playback_mode: Option<String>,
    #[serde(default)]
    pub volume: Option<Value>,
    #[serde(default)]
    pub start_silent: Option<bool>,
    #[serde(default)]
    pub active_conditions: Vec<SceneAudioCueCondition>,
}

impl SceneAudioCue {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        if let Some(resource) = &self.resource {
            validate_required_text(&format!("scene node {node_id:?} audio resource"), resource)?;
        }
        if let Some(source) = &self.source {
            validate_required_text(&format!("scene node {node_id:?} audio source"), source)?;
        }
        if self.resource.is_none() && self.source.is_none() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} audio cue must define resource or source"
            )));
        }
        for condition in &self.active_conditions {
            condition.validate(node_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneAudioCueCondition {
    pub property: String,
    #[serde(default)]
    pub equals: Option<f64>,
}

impl SceneAudioCueCondition {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        validate_required_text(
            &format!("scene node {node_id:?} audio active condition property"),
            &self.property,
        )?;
        validate_optional_finite(
            &format!("scene node {node_id:?} audio active condition equals"),
            self.equals,
        )
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
    Audio,
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
        let rotation = self.rotation_deg.to_radians();
        let child_x = child.x * self.scale_x;
        let child_y = child.y * self.scale_y;
        let rotated_child_x = child_x.mul_add(rotation.cos(), -child_y * rotation.sin());
        let rotated_child_y = child_x.mul_add(rotation.sin(), child_y * rotation.cos());
        Self {
            x: self.x + rotated_child_x,
            y: self.y + rotated_child_y,
            scale_x: self.scale_x * child.scale_x,
            scale_y: self.scale_y * child.scale_y,
            rotation_deg: self.rotation_deg + child.rotation_deg,
            anchor_x: child.anchor_x,
            anchor_y: child.anchor_y,
        }
    }
}

#[inline]
fn scene_compose_particle_transform(
    parent: SceneTransform,
    parent_sin: f64,
    parent_cos: f64,
    x: f64,
    y: f64,
    rotation_deg: f64,
) -> SceneTransform {
    let child_x = x * parent.scale_x;
    let child_y = y * parent.scale_y;
    let rotated_child_x = child_x.mul_add(parent_cos, -child_y * parent_sin);
    let rotated_child_y = child_x.mul_add(parent_sin, child_y * parent_cos);
    SceneTransform {
        x: parent.x + rotated_child_x,
        y: parent.y + rotated_child_y,
        scale_x: parent.scale_x,
        scale_y: parent.scale_y,
        rotation_deg: parent.rotation_deg + rotation_deg,
        anchor_x: 0.5,
        anchor_y: 0.5,
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
    Width,
    Height,
    CornerRadius,
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
        if matches!(
            property,
            SceneAnimatedProperty::Width
                | SceneAnimatedProperty::Height
                | SceneAnimatedProperty::CornerRadius
        ) && self.value < 0.0
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} geometry value must be non-negative"
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
    pub progress_estimate_percent: Option<u8>,
    #[serde(default)]
    pub full_scene_complete: bool,
    #[serde(default)]
    pub completed_boundaries: Vec<String>,
    #[serde(default)]
    pub pending_boundaries: Vec<String>,
    #[serde(default)]
    pub unsupported_boundaries: Vec<String>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SceneParallaxOffset {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneSnapshotVisibility {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneSnapshotBuildOptions {
    compact_particle_ids: bool,
}

impl SceneSnapshotVisibility {
    fn from_size(size: Option<SceneSize>) -> Option<Self> {
        let size = size?;
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some(Self {
            width: f64::from(size.width),
            height: f64::from(size.height),
        })
    }

    fn intersects(self, bounds: SceneSnapshotBounds) -> bool {
        bounds.max_x >= 0.0
            && bounds.max_y >= 0.0
            && bounds.min_x <= self.width
            && bounds.min_y <= self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneSnapshotBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SceneSnapshotBounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn include(&mut self, x: f64, y: f64) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshotLayer {
    pub id: String,
    pub kind: SceneNodeKind,
    pub source: Option<PackagePath>,
    pub texture_region: Option<SceneTextureRegion>,
    pub audio: Vec<SceneAudioCue>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub mesh: Option<SceneMesh>,
    pub parallax_depth: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_source: Option<PackagePath>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub path_fill_rule: ScenePathFillRule,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshotSampledImageLayer {
    pub has_source: bool,
    pub texture_region: Option<SceneTextureRegion>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub mesh: Option<SceneMesh>,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
}

fn scene_snapshot_layer_intersects_visibility(
    layer: &SceneSnapshotLayer,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    scene_snapshot_visual_bounds_intersects(
        layer.width,
        layer.height,
        layer.mesh.as_ref(),
        layer.transform,
        visibility,
    )
}

fn scene_sampled_image_snapshot_layer_intersects_visibility(
    layer: &SceneSnapshotSampledImageLayer,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    scene_snapshot_visual_bounds_intersects(
        layer.width,
        layer.height,
        layer.mesh.as_ref(),
        layer.transform,
        visibility,
    )
}

fn scene_snapshot_visual_bounds_intersects(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    let Some(visibility) = visibility else {
        return true;
    };
    let (Some(width), Some(height)) = (width, height) else {
        return true;
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return true;
    }
    let Some(bounds) = scene_snapshot_visual_bounds(width, height, mesh, transform) else {
        return true;
    };
    visibility.intersects(bounds)
}

fn scene_snapshot_visual_bounds(
    width: f64,
    height: f64,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
) -> Option<SceneSnapshotBounds> {
    let rotation = transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let mut bounds = SceneSnapshotBounds::empty();
    if let Some(mesh) = mesh {
        let local_offset_x = (0.5 - transform.anchor_x) * width;
        let local_offset_y = (0.5 - transform.anchor_y) * height;
        for vertex in &mesh.vertices {
            let (x, y) = scene_snapshot_transform_point(
                vertex.x + local_offset_x,
                vertex.y + local_offset_y,
                transform,
                cos,
                sin,
            )?;
            if !bounds.include(x, y) {
                return None;
            }
        }
    } else {
        let left = -transform.anchor_x * width;
        let top = -transform.anchor_y * height;
        let right = left + width;
        let bottom = top + height;
        for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
            let (x, y) = scene_snapshot_transform_point(x, y, transform, cos, sin)?;
            if !bounds.include(x, y) {
                return None;
            }
        }
    }
    Some(bounds)
}

fn scene_snapshot_transform_point(
    x: f64,
    y: f64,
    transform: SceneTransform,
    cos: f64,
    sin: f64,
) -> Option<(f64, f64)> {
    let scaled_x = x * transform.scale_x;
    let scaled_y = y * transform.scale_y;
    let scene_x = scaled_x.mul_add(cos, -scaled_y * sin) + transform.x;
    let scene_y = scaled_x.mul_add(sin, scaled_y * cos) + transform.y;
    if !scene_x.is_finite() || !scene_y.is_finite() {
        return None;
    }
    Some((scene_x, scene_y))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneMesh {
    #[serde(default)]
    pub vertices: Vec<SceneMeshVertex>,
    #[serde(default)]
    pub indices: Vec<u32>,
}

impl SceneMesh {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        if self.vertices.len() < 3 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh must contain at least 3 vertices"
            )));
        }
        if self.indices.len() < 3 || self.indices.len() % 3 != 0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh indices must contain complete triangles"
            )));
        }
        for (index, vertex) in self.vertices.iter().enumerate() {
            vertex.validate(node_id, index)?;
        }
        let vertex_count = self.vertices.len();
        for index in &self.indices {
            if usize::try_from(*index).map_or(true, |index| index >= vertex_count) {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh index {index} is outside the vertex array"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshVertex {
    pub x: f64,
    pub y: f64,
    pub u: f64,
    pub v: f64,
}

impl SceneMeshVertex {
    fn validate(&self, node_id: &str, index: usize) -> Result<(), SceneError> {
        for (field, value) in [("x", self.x), ("y", self.y), ("u", self.u), ("v", self.v)] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh vertex {index} {field} must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenePathFillRule {
    #[default]
    Nonzero,
    Evenodd,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTextureRegion {
    pub u_min: f64,
    pub v_min: f64,
    pub u_max: f64,
    pub v_max: f64,
    pub frame_index: u32,
    pub frame_count: u32,
    #[serde(default)]
    pub columns: u32,
    #[serde(default)]
    pub rows: u32,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default = "default_scene_texture_region_loop_playback")]
    pub loop_playback: bool,
}

impl SceneTextureRegion {
    fn validate(self) -> Option<Self> {
        if self.u_min.is_finite()
            && self.v_min.is_finite()
            && self.u_max.is_finite()
            && self.v_max.is_finite()
            && self.u_min >= 0.0
            && self.v_min >= 0.0
            && self.u_max <= 1.0
            && self.v_max <= 1.0
            && self.u_min < self.u_max
            && self.v_min < self.v_max
            && self.frame_count > 0
            && self.frame_index < self.frame_count
            && self.columns > 0
            && self.rows > 0
            && self.fps.is_none_or(|fps| fps.is_finite() && fps > 0.0)
        {
            Some(self)
        } else {
            None
        }
    }
}

fn default_scene_texture_region_loop_playback() -> bool {
    true
}

fn scene_texture_region_from_properties(
    properties: &BTreeMap<String, Value>,
    time_ms: u64,
) -> Option<SceneTextureRegion> {
    let spritesheet = properties.get("spritesheet")?.as_object()?;
    let atlas_width = scene_property_u32(spritesheet, "atlas_width")?;
    let atlas_height = scene_property_u32(spritesheet, "atlas_height")?;
    let frame_width = scene_property_u32(spritesheet, "frame_width")?;
    let frame_height = scene_property_u32(spritesheet, "frame_height")?;
    let columns = scene_property_u32(spritesheet, "columns").unwrap_or_else(|| {
        if frame_width == 0 {
            0
        } else {
            atlas_width / frame_width
        }
    });
    let rows = scene_property_u32(spritesheet, "rows").unwrap_or_else(|| {
        if frame_height == 0 {
            0
        } else {
            atlas_height / frame_height
        }
    });
    let frame_count = scene_property_u32(spritesheet, "frame_count")
        .unwrap_or_else(|| columns.saturating_mul(rows));
    if atlas_width == 0
        || atlas_height == 0
        || frame_width == 0
        || frame_height == 0
        || columns == 0
        || rows == 0
        || frame_count == 0
    {
        return None;
    }
    let max_frames = columns.saturating_mul(rows);
    let frame_count = frame_count.min(max_frames);
    if frame_count == 0 {
        return None;
    }
    let fps = scene_property_f64(spritesheet, "fps")
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(12.0);
    let loop_playback = scene_property_bool(spritesheet, "loop").unwrap_or(true);
    let frame = ((time_ms as f64 / 1000.0) * fps).floor();
    let frame_index = if frame.is_finite() && frame >= 0.0 {
        let frame = frame as u64;
        if loop_playback {
            (frame % u64::from(frame_count)) as u32
        } else {
            frame.min(u64::from(frame_count - 1)) as u32
        }
    } else {
        0
    };
    let column = frame_index % columns;
    let row = frame_index / columns;
    SceneTextureRegion {
        u_min: f64::from(column * frame_width) / f64::from(atlas_width),
        v_min: f64::from(row * frame_height) / f64::from(atlas_height),
        u_max: f64::from((column + 1) * frame_width) / f64::from(atlas_width),
        v_max: f64::from((row + 1) * frame_height) / f64::from(atlas_height),
        frame_index,
        frame_count,
        columns,
        rows,
        fps: Some(fps),
        loop_playback,
    }
    .validate()
}

fn scene_text_from_properties(
    properties: &BTreeMap<String, Value>,
    resolve_text_property: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let property = properties
        .get("text_binding")
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("property"))
        .and_then(Value::as_str)?;
    resolve_scene_text_property(resolve_text_property, property)
}

fn scene_audio_cues_for_snapshot(
    cues: &[SceneAudioCue],
    resolve_property: &impl Fn(&str) -> Option<f64>,
) -> Vec<SceneAudioCue> {
    cues.iter()
        .filter_map(|cue| {
            if cue.active_conditions.is_empty() {
                return Some(cue.clone());
            }
            scene_audio_cue_conditions_active(&cue.active_conditions, resolve_property).then(|| {
                let mut cue = cue.clone();
                cue.start_silent = Some(false);
                cue
            })
        })
        .collect()
}

fn scene_audio_cue_conditions_active(
    conditions: &[SceneAudioCueCondition],
    resolve_property: &impl Fn(&str) -> Option<f64>,
) -> bool {
    conditions.iter().all(|condition| {
        let Some(value) = resolve_scene_property(resolve_property, &[condition.property.as_str()])
        else {
            return false;
        };
        if let Some(expected) = condition.equals {
            (value - expected).abs() <= f64::EPSILON
        } else {
            value > 0.0
        }
    })
}

fn scene_property_u32(object: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    match object.get(key)? {
        Value::Number(value) => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_property_f64(object: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
    match object.get(key)? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_property_bool(object: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    match object.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn scene_particle_value<'a>(
    object: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a Value> {
    let value = object?.get(key)?;
    Some(value.get("value").unwrap_or(value))
}

fn scene_particle_f64(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<f64> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_u32(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<u32> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_u64(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<u64> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_bool(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<bool> {
    match scene_particle_value(object, key)? {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn scene_particle_string(
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<String> {
    match scene_particle_value(object, key)? {
        Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn scene_particle_seed_from_id(id: &str) -> u64 {
    let mut seed = 0xcbf29ce484222325u64;
    for byte in id.as_bytes() {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x100000001b3);
    }
    seed
}

#[inline]
fn scene_particle_unit(seed: u64, index: u32, salt: u64) -> f64 {
    let mut value = seed
        ^ (u64::from(index).wrapping_mul(0x9e3779b97f4a7c15))
        ^ salt.wrapping_mul(0xbf58476d1ce4e5b9);
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^= value >> 31;
    ((value >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

fn apply_scene_animated_value(
    transform: &mut SceneTransform,
    opacity: &mut f64,
    width: &mut Option<f64>,
    height: &mut Option<f64>,
    corner_radius: &mut Option<f64>,
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
        SceneAnimatedProperty::Width => *width = Some(value.max(0.0)),
        SceneAnimatedProperty::Height => *height = Some(value.max(0.0)),
        SceneAnimatedProperty::CornerRadius => *corner_radius = Some(value.max(0.0)),
        SceneAnimatedProperty::Custom => {}
    }
}

fn resolve_scene_property(
    resolve_property: &impl Fn(&str) -> Option<f64>,
    names: &[&str],
) -> Option<f64> {
    names
        .iter()
        .filter_map(|name| resolve_property(name))
        .find(|value| value.is_finite())
}

fn resolve_scene_text_property(
    resolve_text_property: &impl Fn(&str) -> Option<String>,
    property: &str,
) -> Option<String> {
    let property = property.trim();
    if property.is_empty() {
        None
    } else {
        resolve_text_property(property).filter(|text| !text.is_empty())
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

fn validate_optional_finite(field: &str, value: Option<f64>) -> Result<(), SceneError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(SceneError::invalid(format!("{field} must be finite")));
    }
    Ok(())
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
                "progress_estimate_percent": 100,
                "full_scene_complete": true,
                "unsupported_boundaries": ["cursor-parallax-input-source"]
            }
        }))
        .unwrap();

        document.validate().unwrap();
        assert_eq!(
            document.referenced_paths(),
            vec![
                PackagePath::new("metadata/source-scene.json").unwrap(),
                PackagePath::new("assets/scene-resources/background.png").unwrap(),
            ]
        );
        assert_eq!(
            document.native_lowering.progress_estimate_percent,
            Some(100)
        );
        assert!(document.native_lowering.full_scene_complete);
        assert_eq!(
            document.native_lowering.unsupported_boundaries,
            vec!["cursor-parallax-input-source".to_owned()]
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

    #[test]
    fn render_clear_color_becomes_first_snapshot_layer() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 320, "height": 180 },
            "render": {
                "clear_color": "#102030",
                "clear_enabled": true
            },
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "width": 50,
                    "height": 25
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].id, "scene-render-clear-color");
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Color);
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#102030"));
        assert_eq!(snapshot.layers[0].width, Some(320.0));
        assert_eq!(snapshot.layers[0].height, Some(180.0));
        assert_eq!(snapshot.layers[1].id, "node-panel");
    }

    #[test]
    fn text_binding_resolver_overrides_static_snapshot_text() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-clock",
                    "type": "text",
                    "text": "12:34",
                    "properties": {
                        "text_binding": {
                            "property": "scene.clock.local.strftime:%H:%M"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let static_snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(static_snapshot.layers[0].text.as_deref(), Some("12:34"));

        let dynamic_snapshot = document.snapshot_at_with_resolvers(
            0,
            |_| None,
            |property| (property == "scene.clock.local.strftime:%H:%M").then(|| "23:45".to_owned()),
        );
        assert_eq!(dynamic_snapshot.layers[0].text.as_deref(), Some("23:45"));
    }

    #[test]
    fn audio_cue_active_conditions_filter_snapshot_audio() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-audio",
                    "type": "audio",
                    "audio": [
                        {
                            "source": "voice.mp3",
                            "start_silent": true,
                            "active_conditions": [
                                { "property": "scene.controller.idle.active" },
                                { "property": "voice_enabled", "equals": 1.0 }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let inactive = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.controller.idle.active" => Some(1.0),
            "voice_enabled" => Some(0.0),
            _ => None,
        });
        assert!(inactive.layers[0].audio.is_empty());

        let active = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.controller.idle.active" => Some(1.0),
            "voice_enabled" => Some(1.0),
            _ => None,
        });
        assert_eq!(active.layers[0].audio.len(), 1);
        assert_eq!(active.layers[0].audio[0].start_silent, Some(false));
    }

    #[test]
    fn disabled_render_clear_color_does_not_emit_snapshot_layer() {
        let document: SceneDocument = serde_json::from_value(json!({
            "render": {
                "clear_color": "#102030",
                "clear_enabled": false
            },
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff"
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "node-panel");
    }

    #[test]
    fn timeline_and_property_bindings_drive_scene_geometry_fields() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "node-panel",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "width": 100,
                    "height": 50,
                    "corner_radius": 4
                }
            ],
            "timelines": [
                {
                    "id": "panel-size",
                    "target_node": "node-panel",
                    "channels": [
                        {
                            "property": "width",
                            "keyframes": [
                                { "time_ms": 0, "value": 100 },
                                { "time_ms": 1000, "value": 200 }
                            ]
                        },
                        {
                            "property": "height",
                            "keyframes": [
                                { "time_ms": 0, "value": 50 },
                                { "time_ms": 1000, "value": 150 }
                            ]
                        }
                    ]
                }
            ],
            "property_bindings": [
                {
                    "property": "panel_radius",
                    "target_node": "node-panel",
                    "target": "corner-radius"
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |property| {
            if property == "panel_radius" {
                Some(12.0)
            } else {
                None
            }
        });
        assert_eq!(snapshot.layers[0].width, Some(150.0));
        assert_eq!(snapshot.layers[0].height, Some(100.0));
        assert_eq!(snapshot.layers[0].corner_radius, Some(12.0));
    }

    #[test]
    fn spritesheet_properties_drive_time_sampled_texture_region() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-atlas",
                    "type": "image",
                    "source": "assets/atlas.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "node-atlas",
                    "type": "image",
                    "resource": "resource-atlas",
                    "properties": {
                        "spritesheet": {
                            "type": "atlas-grid",
                            "atlas_width": 300,
                            "atlas_height": 400,
                            "frame_width": 100,
                            "frame_height": 100,
                            "columns": 3,
                            "rows": 4,
                            "frame_count": 12,
                            "fps": 12,
                            "loop": true
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let first = document.snapshot_at_with_property_resolver(0, |_| None);
        assert_eq!(
            first.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 1.0 / 3.0,
                v_max: 0.25,
                frame_index: 0,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            })
        );

        let sixth = document.snapshot_at_with_property_resolver(417, |_| None);
        assert_eq!(
            sixth.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 2.0 / 3.0,
                v_min: 0.25,
                u_max: 1.0,
                v_max: 0.5,
                frame_index: 5,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            })
        );
    }

    #[test]
    fn parallax_properties_offset_node_transforms_by_depth() {
        let document: SceneDocument = serde_json::from_value(json!({
            "render": {
                "parallax": { "amount": 10 }
            },
            "nodes": [
                {
                    "id": "near",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "transform": { "x": 3, "y": 4 },
                    "parallax_depth": 0.5
                },
                {
                    "id": "flat",
                    "type": "rectangle",
                    "color": "#ffffff",
                    "transform": { "x": 1, "y": 2 }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |property| match property {
            "scene.parallax.x" => Some(2.0),
            "scene.parallax.y" => Some(-1.0),
            _ => None,
        });
        assert_eq!(snapshot.layers[0].transform.x, 13.0);
        assert_eq!(snapshot.layers[0].transform.y, -1.0);
        assert_eq!(snapshot.layers[0].parallax_depth, Some(0.5));
        assert_eq!(snapshot.layers[1].transform.x, 1.0);
        assert_eq!(snapshot.layers[1].transform.y, 2.0);
    }

    #[test]
    fn parent_rotation_offsets_child_transform_coordinates() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "rotating-parent",
                    "type": "group",
                    "transform": {
                        "x": 10,
                        "y": 20,
                        "scale_x": 2,
                        "scale_y": 3,
                        "rotation_deg": 90
                    },
                    "children": [
                        {
                            "id": "child-panel",
                            "type": "rectangle",
                            "color": "#ffffff",
                            "transform": {
                                "x": 5,
                                "y": 2,
                                "rotation_deg": 15
                            }
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(0, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "child-panel");
        assert!((snapshot.layers[0].transform.x - 4.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.y - 30.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.rotation_deg - 105.0).abs() < f64::EPSILON);
        assert!((snapshot.layers[0].transform.scale_x - 2.0).abs() < f64::EPSILON);
        assert!((snapshot.layers[0].transform.scale_y - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn particle_emitter_expands_to_native_rectangle_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "spark-emitter",
                    "type": "particle-emitter",
                    "opacity": 0.5,
                    "transform": { "x": 50, "y": 25 },
                    "properties": {
                        "particle": {
                            "count": 3,
                            "seed": 11,
                            "lifetime_ms": 1000,
                            "size": 12,
                            "speed": 0,
                            "spawn_width": 0,
                            "spawn_height": 0,
                            "fade": false,
                            "color": "#ffaa00"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(250, |_| None);

        assert_eq!(snapshot.layers.len(), 3);
        assert_eq!(snapshot.layers[0].id, "spark-emitter::particle-0");
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Rectangle);
        assert_eq!(snapshot.layers[0].color.as_deref(), Some("#ffaa00"));
        assert_eq!(snapshot.layers[0].width, Some(12.0));
        assert_eq!(snapshot.layers[0].height, Some(12.0));
        assert_eq!(snapshot.layers[0].opacity, 0.5);
        assert_eq!(snapshot.layers[0].transform.x, 50.0);
        assert_eq!(snapshot.layers[0].transform.y, 25.0);
        assert!(
            snapshot
                .layers
                .iter()
                .all(|layer| layer.kind != SceneNodeKind::ParticleEmitter)
        );
    }

    #[test]
    fn particle_emitter_with_resource_expands_to_sampled_image_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                {
                    "id": "resource-spark",
                    "type": "image",
                    "source": "assets/scene-resources/spark.gtex"
                }
            ],
            "nodes": [
                {
                    "id": "spark-emitter",
                    "type": "particle-emitter",
                    "resource": "resource-spark",
                    "properties": {
                        "particle": {
                            "count": 2,
                            "seed": 11,
                            "lifetime_ms": 1000,
                            "size": 12,
                            "speed": 0,
                            "spawn_width": 0,
                            "spawn_height": 0,
                            "fade": false
                        },
                        "spritesheet": {
                            "type": "atlas-grid",
                            "atlas_width": 64,
                            "atlas_height": 32,
                            "frame_width": 32,
                            "frame_height": 32,
                            "columns": 2,
                            "rows": 1,
                            "frame_count": 2,
                            "fps": 2,
                            "loop": true
                        }
                    }
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(500, |_| None);

        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].kind, SceneNodeKind::Image);
        assert_eq!(
            snapshot.layers[0].source,
            Some(PackagePath::new("assets/scene-resources/spark.gtex").unwrap())
        );
        assert_eq!(
            snapshot.layers[0].texture_region,
            Some(SceneTextureRegion {
                u_min: 0.5,
                v_min: 0.0,
                u_max: 1.0,
                v_max: 1.0,
                frame_index: 1,
                frame_count: 2,
                columns: 2,
                rows: 1,
                fps: Some(2.0),
                loop_playback: true,
            })
        );
        assert!(snapshot.layers.iter().all(|layer| layer.source.is_some()));
    }

    #[test]
    fn particle_emitter_inherits_rotated_parent_transform() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "rotating-parent",
                    "type": "group",
                    "transform": { "x": 10, "y": 20, "rotation_deg": 90 },
                    "children": [
                        {
                            "id": "spark-emitter",
                            "type": "particle-emitter",
                            "transform": { "x": 5, "y": 0 },
                            "properties": {
                                "particle": {
                                    "count": 1,
                                    "seed": 11,
                                    "lifetime_ms": 1000,
                                    "size": 12,
                                    "speed": 0,
                                    "spawn_width": 0,
                                    "spawn_height": 0,
                                    "fade": false
                                }
                            }
                        }
                    ]
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at_with_property_resolver(250, |_| None);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "spark-emitter::particle-0");
        assert!((snapshot.layers[0].transform.x - 10.0).abs() < 0.000001);
        assert!((snapshot.layers[0].transform.y - 25.0).abs() < 0.000001);
    }
}
