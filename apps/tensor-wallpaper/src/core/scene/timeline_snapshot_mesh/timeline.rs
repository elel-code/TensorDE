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
    #[serde(default, skip_serializing_if = "is_default")]
    pub time_offset_ms: u64,
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
        let time_ms = time_ms.saturating_add(self.time_offset_ms);
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
pub struct SceneShaderLowering {
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
