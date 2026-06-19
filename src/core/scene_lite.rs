use super::manifest::FitMode;
use super::path::PackagePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

const SCENE_LITE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLiteDocument {
    #[serde(default = "default_scene_lite_version")]
    pub version: u32,
    #[serde(default)]
    pub size: Option<SceneLiteSize>,
    #[serde(default)]
    pub layers: Vec<SceneLiteLayer>,
    #[serde(default)]
    pub property_bindings: Vec<SceneLitePropertyBinding>,
}

impl SceneLiteDocument {
    pub fn validate(&self) -> Result<(), SceneLiteError> {
        if self.version != SCENE_LITE_VERSION {
            return Err(SceneLiteError::invalid(format!(
                "unsupported scene-lite version {}; supported version is {}",
                self.version, SCENE_LITE_VERSION
            )));
        }
        if let Some(size) = self.size {
            size.validate()?;
        }
        let mut layer_ids = BTreeSet::new();
        for binding in &self.property_bindings {
            binding.validate("document")?;
        }
        for layer in &self.layers {
            layer.validate(&mut layer_ids)?;
        }
        Ok(())
    }

    pub fn referenced_paths(&self) -> Vec<PackagePath> {
        let mut paths = Vec::new();
        for layer in &self.layers {
            layer.push_referenced_paths(&mut paths);
        }
        paths
    }

    pub fn snapshot_at(&self, time_ms: u64) -> SceneLiteSnapshot {
        let mut layers = Vec::new();
        for layer in &self.layers {
            layer.push_snapshot_layers(time_ms, SceneLiteTransform::default(), 1.0, &mut layers);
        }
        SceneLiteSnapshot { time_ms, layers }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneLiteSize {
    pub width: u32,
    pub height: u32,
}

impl SceneLiteSize {
    fn validate(self) -> Result<(), SceneLiteError> {
        if self.width == 0 || self.height == 0 {
            return Err(SceneLiteError::invalid(
                "scene-lite size width and height must be greater than 0",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLiteLayer {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: SceneLiteLayerKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub transform: SceneLiteTransform,
    #[serde(default)]
    pub source: Option<PackagePath>,
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
    pub text_align: Option<SceneLiteTextAlign>,
    #[serde(default)]
    pub fit: FitMode,
    #[serde(default)]
    pub animations: Vec<SceneLiteAnimation>,
    #[serde(default)]
    pub property_bindings: Vec<SceneLitePropertyBinding>,
    #[serde(default)]
    pub layers: Vec<SceneLiteLayer>,
}

impl SceneLiteLayer {
    fn validate(&self, layer_ids: &mut BTreeSet<String>) -> Result<(), SceneLiteError> {
        validate_required_text("scene-lite layer id", &self.id)?;
        if !layer_ids.insert(self.id.clone()) {
            return Err(SceneLiteError::invalid(format!(
                "duplicate scene-lite layer id {:?}",
                self.id
            )));
        }
        validate_opacity(self.opacity, &self.id)?;
        self.transform.validate(&self.id)?;
        match self.kind {
            SceneLiteLayerKind::Image => {
                if self.source.is_none() {
                    return Err(SceneLiteError::invalid(format!(
                        "image layer {:?} must define source",
                        self.id
                    )));
                }
            }
            SceneLiteLayerKind::Color => {
                if self.color.as_deref().is_none_or(str::is_empty) {
                    return Err(SceneLiteError::invalid(format!(
                        "color layer {:?} must define color",
                        self.id
                    )));
                }
            }
            SceneLiteLayerKind::Rectangle | SceneLiteLayerKind::Ellipse => {
                if self.color.as_deref().is_none_or(str::is_empty) {
                    return Err(SceneLiteError::invalid(format!(
                        "{:?} layer {:?} must define color",
                        self.kind, self.id
                    )));
                }
                self.validate_shape_fields()?;
            }
            SceneLiteLayerKind::Text => {
                if self.text.as_deref().is_none_or(str::is_empty) {
                    return Err(SceneLiteError::invalid(format!(
                        "text layer {:?} must define text",
                        self.id
                    )));
                }
                if self.color.as_deref().is_none_or(str::is_empty) {
                    return Err(SceneLiteError::invalid(format!(
                        "text layer {:?} must define color",
                        self.id
                    )));
                }
                self.validate_text_fields()?;
            }
            SceneLiteLayerKind::Group => {}
        }
        for animation in &self.animations {
            animation.validate(&self.id)?;
        }
        for binding in &self.property_bindings {
            binding.validate(&self.id)?;
        }
        for layer in &self.layers {
            layer.validate(layer_ids)?;
        }
        Ok(())
    }

    fn push_referenced_paths(&self, paths: &mut Vec<PackagePath>) {
        if self.kind == SceneLiteLayerKind::Image
            && let Some(source) = &self.source
        {
            paths.push(source.clone());
        }
        for layer in &self.layers {
            layer.push_referenced_paths(paths);
        }
    }

    fn push_snapshot_layers(
        &self,
        time_ms: u64,
        parent_transform: SceneLiteTransform,
        parent_opacity: f64,
        output: &mut Vec<SceneLiteSnapshotLayer>,
    ) {
        if !self.visible {
            return;
        }

        let mut transform = self.transform;
        let mut opacity = self.opacity;
        for animation in &self.animations {
            let value = animation.value_at(time_ms);
            match animation.property {
                SceneLiteAnimatedProperty::Opacity => opacity = value,
                SceneLiteAnimatedProperty::X => transform.x = value,
                SceneLiteAnimatedProperty::Y => transform.y = value,
                SceneLiteAnimatedProperty::ScaleX => transform.scale_x = value,
                SceneLiteAnimatedProperty::ScaleY => transform.scale_y = value,
                SceneLiteAnimatedProperty::RotationDeg => transform.rotation_deg = value,
            }
        }

        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        if self.kind != SceneLiteLayerKind::Group {
            output.push(SceneLiteSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: self.source.clone(),
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
                fit: self.fit,
                opacity,
                transform,
            });
        }
        for layer in &self.layers {
            layer.push_snapshot_layers(time_ms, transform, opacity, output);
        }
    }

    fn validate_shape_fields(&self) -> Result<(), SceneLiteError> {
        for (field, value) in [
            ("stroke_width", self.stroke_width),
            ("corner_radius", self.corner_radius),
        ] {
            if let Some(value) = value
                && (!value.is_finite() || value < 0.0)
            {
                return Err(SceneLiteError::invalid(format!(
                    "layer {:?} {field} must be finite and greater than or equal to 0",
                    self.id
                )));
            }
        }
        for (field, value) in [("width", self.width), ("height", self.height)] {
            if let Some(value) = value
                && (!value.is_finite() || value <= 0.0)
            {
                return Err(SceneLiteError::invalid(format!(
                    "layer {:?} {field} must be finite and greater than 0",
                    self.id
                )));
            }
        }
        Ok(())
    }

    fn validate_text_fields(&self) -> Result<(), SceneLiteError> {
        if let Some(font_size) = self.font_size
            && (!font_size.is_finite() || font_size <= 0.0)
        {
            return Err(SceneLiteError::invalid(format!(
                "layer {:?} font_size must be finite and greater than 0",
                self.id
            )));
        }
        for (field, value) in [("width", self.width), ("height", self.height)] {
            if let Some(value) = value
                && (!value.is_finite() || value <= 0.0)
            {
                return Err(SceneLiteError::invalid(format!(
                    "layer {:?} {field} must be finite and greater than 0",
                    self.id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneLiteLayerKind {
    Image,
    Color,
    Rectangle,
    Ellipse,
    Text,
    Group,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneLiteTextAlign {
    #[default]
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneLiteTransform {
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

impl Default for SceneLiteTransform {
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

impl SceneLiteTransform {
    fn validate(self, layer_id: &str) -> Result<(), SceneLiteError> {
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
                return Err(SceneLiteError::invalid(format!(
                    "layer {layer_id:?} transform {field} must be finite"
                )));
            }
        }
        if self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return Err(SceneLiteError::invalid(format!(
                "layer {layer_id:?} transform scale values must be greater than 0"
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
pub struct SceneLiteAnimation {
    pub property: SceneLiteAnimatedProperty,
    #[serde(rename = "loop", default)]
    pub loop_playback: bool,
    pub keyframes: Vec<SceneLiteKeyframe>,
}

impl SceneLiteAnimation {
    fn validate(&self, layer_id: &str) -> Result<(), SceneLiteError> {
        if self.keyframes.is_empty() {
            return Err(SceneLiteError::invalid(format!(
                "layer {layer_id:?} animation {:?} must define keyframes",
                self.property
            )));
        }
        let mut previous = None;
        for keyframe in &self.keyframes {
            keyframe.validate(layer_id, self.property)?;
            if previous.is_some_and(|previous| keyframe.time_ms <= previous) {
                return Err(SceneLiteError::invalid(format!(
                    "layer {layer_id:?} animation {:?} keyframes must use increasing time_ms",
                    self.property
                )));
            }
            previous = Some(keyframe.time_ms);
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
pub enum SceneLiteAnimatedProperty {
    Opacity,
    X,
    Y,
    ScaleX,
    ScaleY,
    RotationDeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneLiteKeyframe {
    pub time_ms: u64,
    pub value: f64,
    #[serde(default)]
    pub curve: SceneLiteCurve,
}

impl SceneLiteKeyframe {
    fn validate(
        self,
        layer_id: &str,
        property: SceneLiteAnimatedProperty,
    ) -> Result<(), SceneLiteError> {
        if !self.value.is_finite() {
            return Err(SceneLiteError::invalid(format!(
                "layer {layer_id:?} animation {property:?} keyframe value must be finite"
            )));
        }
        if property == SceneLiteAnimatedProperty::Opacity {
            validate_opacity(self.value, layer_id)?;
        }
        if matches!(
            property,
            SceneLiteAnimatedProperty::ScaleX | SceneLiteAnimatedProperty::ScaleY
        ) && self.value <= 0.0
        {
            return Err(SceneLiteError::invalid(format!(
                "layer {layer_id:?} animation {property:?} scale value must be greater than 0"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneLiteCurve {
    #[default]
    Linear,
    Step,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl SceneLiteCurve {
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
pub struct SceneLitePropertyBinding {
    pub property: String,
    pub target: SceneLiteAnimatedProperty,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
}

impl SceneLitePropertyBinding {
    fn validate(&self, owner: &str) -> Result<(), SceneLiteError> {
        validate_required_text("scene-lite property binding property", &self.property)?;
        if let Some(layer) = &self.layer {
            validate_required_text("scene-lite property binding layer", layer)?;
        }
        for (field, value) in [("scale", self.scale), ("offset", self.offset)] {
            if let Some(value) = value
                && !value.is_finite()
            {
                return Err(SceneLiteError::invalid(format!(
                    "scene-lite property binding {field} on {owner:?} must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneLiteSnapshot {
    pub time_ms: u64,
    pub layers: Vec<SceneLiteSnapshotLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneLiteSnapshotLayer {
    pub id: String,
    pub kind: SceneLiteLayerKind,
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
    pub text_align: Option<SceneLiteTextAlign>,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneLiteError {
    message: String,
}

impl SceneLiteError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SceneLiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SceneLiteError {}

fn validate_required_text(field: &str, value: &str) -> Result<(), SceneLiteError> {
    if value.trim().is_empty() {
        Err(SceneLiteError::invalid(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn validate_opacity(value: f64, layer_id: &str) -> Result<(), SceneLiteError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(SceneLiteError::invalid(format!(
            "layer {layer_id:?} opacity must be between 0 and 1"
        )))
    }
}

fn default_scene_lite_version() -> u32 {
    SCENE_LITE_VERSION
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

fn default_scale() -> f64 {
    1.0
}

fn default_anchor() -> f64 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_scene_lite_layers() {
        let document: SceneLiteDocument = serde_json::from_str(
            r##"
            {
              "version": 1,
              "size": { "width": 1920, "height": 1080 },
              "layers": [
                {
                  "id": "background",
                  "type": "image",
                  "source": "assets/background.png",
                  "fit": "cover",
                  "opacity": 0.75,
                  "transform": { "x": 10, "y": 20, "scale_x": 1.5 },
                  "animations": [
                    {
                      "property": "opacity",
                      "loop": true,
                      "keyframes": [
                        { "time_ms": 0, "value": 0.25 },
                        { "time_ms": 1000, "value": 1.0, "curve": "linear" }
                      ]
                    }
                  ]
                }
              ]
            }
            "##,
        )
        .unwrap();

        document.validate().unwrap();
        assert_eq!(document.referenced_paths().len(), 1);
        let snapshot = document.snapshot_at(500);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "background");
        assert_eq!(snapshot.layers[0].fit, FitMode::Cover);
        assert_eq!(snapshot.layers[0].transform.x, 10.0);
        assert!((snapshot.layers[0].opacity - 0.625).abs() < f64::EPSILON);
    }

    #[test]
    fn composes_group_transform_and_opacity() {
        let document: SceneLiteDocument = serde_json::from_str(
            r##"
            {
              "layers": [
                {
                  "id": "group",
                  "type": "group",
                  "opacity": 0.5,
                  "transform": { "x": 100, "y": 50, "scale_x": 2, "scale_y": 3 },
                  "layers": [
                    {
                      "id": "child",
                      "type": "color",
                      "color": "#ff00ff",
                      "opacity": 0.5,
                      "transform": { "x": 10, "y": 4 }
                    }
                  ]
                }
              ]
            }
            "##,
        )
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at(0);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].id, "child");
        assert_eq!(snapshot.layers[0].opacity, 0.25);
        assert_eq!(snapshot.layers[0].transform.x, 120.0);
        assert_eq!(snapshot.layers[0].transform.y, 62.0);
    }

    #[test]
    fn parses_shape_layers() {
        let document: SceneLiteDocument = serde_json::from_str(
            r##"
            {
              "layers": [
                {
                  "id": "panel",
                  "type": "rectangle",
                  "color": "#102030",
                  "stroke_color": "#ffffff",
                  "stroke_width": 2,
                  "corner_radius": 12,
                  "width": 640,
                  "height": 360
                },
                {
                  "id": "glow",
                  "type": "ellipse",
                  "color": "#80ffaa",
                  "width": 240,
                  "height": 160,
                  "opacity": 0.5
                }
              ]
            }
            "##,
        )
        .unwrap();

        document.validate().unwrap();
        assert!(document.referenced_paths().is_empty());
        let snapshot = document.snapshot_at(0);

        assert_eq!(snapshot.layers.len(), 2);
        assert_eq!(snapshot.layers[0].kind, SceneLiteLayerKind::Rectangle);
        assert_eq!(snapshot.layers[0].stroke_color.as_deref(), Some("#ffffff"));
        assert_eq!(snapshot.layers[0].corner_radius, Some(12.0));
        assert_eq!(snapshot.layers[0].width, Some(640.0));
        assert_eq!(snapshot.layers[1].kind, SceneLiteLayerKind::Ellipse);
        assert_eq!(snapshot.layers[1].height, Some(160.0));
        assert_eq!(snapshot.layers[1].opacity, 0.5);
    }

    #[test]
    fn parses_text_layer() {
        let document: SceneLiteDocument = serde_json::from_str(
            r##"
            {
              "layers": [
                {
                  "id": "title",
                  "type": "text",
                  "text": "Gilder",
                  "color": "#f0f4ff",
                  "font_size": 48,
                  "font_family": "Inter",
                  "font_weight": "700",
                  "text_align": "middle",
                  "width": 640
                }
              ]
            }
            "##,
        )
        .unwrap();

        document.validate().unwrap();
        let snapshot = document.snapshot_at(0);

        assert_eq!(snapshot.layers.len(), 1);
        assert_eq!(snapshot.layers[0].kind, SceneLiteLayerKind::Text);
        assert_eq!(snapshot.layers[0].text.as_deref(), Some("Gilder"));
        assert_eq!(snapshot.layers[0].font_size, Some(48.0));
        assert_eq!(snapshot.layers[0].font_family.as_deref(), Some("Inter"));
        assert_eq!(snapshot.layers[0].font_weight.as_deref(), Some("700"));
        assert_eq!(
            snapshot.layers[0].text_align,
            Some(SceneLiteTextAlign::Middle)
        );
    }

    #[test]
    fn rejects_missing_image_source_and_duplicate_ids() {
        let missing_source: SceneLiteDocument =
            serde_json::from_str(r#"{"layers":[{"id":"a","type":"image"}]}"#).unwrap();
        assert!(missing_source.validate().is_err());

        let duplicate_ids: SceneLiteDocument = serde_json::from_str(
            r##"
            {
              "layers": [
                { "id": "a", "type": "color", "color": "#000000" },
                { "id": "a", "type": "color", "color": "#ffffff" }
              ]
            }
            "##,
        )
        .unwrap();
        assert!(duplicate_ids.validate().is_err());

        let invalid_shape: SceneLiteDocument =
            serde_json::from_str(r#"{"layers":[{"id":"panel","type":"rectangle","width":0}]}"#)
                .unwrap();
        assert!(invalid_shape.validate().is_err());

        let invalid_text: SceneLiteDocument =
            serde_json::from_str(r##"{"layers":[{"id":"title","type":"text","color":"#fff"}]}"##)
                .unwrap();
        assert!(invalid_text.validate().is_err());
    }
}
