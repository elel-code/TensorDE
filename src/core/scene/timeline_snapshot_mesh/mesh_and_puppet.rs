#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneMesh {
    #[serde(default)]
    pub vertices: Vec<SceneMeshVertex>,
    #[serde(default)]
    pub indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SceneMeshSkin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clips: Vec<ScenePuppetAnimationClip>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clipping_records: Vec<SceneMeshPuppetClippingRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clipping_active_sources: Vec<SceneMeshPuppetClippingActiveSource>,
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
        if let Some(skin) = &self.skin {
            skin.validate(node_id, self.vertices.len())?;
        } else if !self.puppet_clips.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh has puppet clips without skin"
            )));
        }
        if !self.puppet_clips.is_empty() {
            let Some(skin) = &self.skin else {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clips without skin"
                )));
            };
            for clip in &self.puppet_clips {
                clip.validate(node_id, skin.bones.len())?;
            }
        }
        if !self.puppet_clipping_records.is_empty() {
            let Some(skin) = &self.skin else {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clipping records without skin"
                )));
            };
            for (index, record) in self.puppet_clipping_records.iter().enumerate() {
                record.validate(node_id, index, skin.bones.len())?;
            }
        }
        if !self.puppet_clipping_active_sources.is_empty() {
            if self.skin.is_none() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clipping active sources without skin"
                )));
            }
            for (index, source) in self.puppet_clipping_active_sources.iter().enumerate() {
                source.validate(node_id, index)?;
            }
        }
        Ok(())
    }

    pub(crate) fn sample_puppet_animation(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<SceneMesh> {
        let mut vertices = Vec::with_capacity(self.vertices.len());
        self.sample_puppet_animation_vertices_into(layers, time_ms, &mut vertices)?;
        Some(SceneMesh {
            vertices,
            indices: self.indices.clone(),
            skin: self.skin.clone(),
            puppet_clips: Vec::new(),
            puppet_clipping_records: self.puppet_clipping_records.clone(),
            puppet_clipping_active_sources: self.puppet_clipping_active_sources.clone(),
        })
    }

    pub(crate) fn sample_puppet_animation_vertices_into(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let inverse_bind_world = self.puppet_inverse_bind_world()?;
        self.sample_puppet_animation_vertices_with_inverse_bind_into(
            layers,
            time_ms,
            &inverse_bind_world,
            vertices,
        )
    }

    pub(crate) fn puppet_inverse_bind_world(&self) -> Option<Vec<[f64; 16]>> {
        let skin = self.skin.as_ref()?;
        Self::puppet_inverse_bind_world_for_skin(skin)
    }

    pub(crate) fn puppet_inverse_bind_world_for_skin(
        skin: &SceneMeshSkin,
    ) -> Option<Vec<[f64; 16]>> {
        let bind_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            skin.bones.iter().map(|bone| bone.bind.matrix()),
        )?;
        bind_world
            .iter()
            .map(|matrix| scene_puppet_inverse_affine_matrix(*matrix))
            .collect::<Option<Vec<_>>>()
    }

    pub(crate) fn sample_puppet_animation_vertices_with_inverse_bind_into(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        self.sample_puppet_animation_vertices_with_clips_and_inverse_bind_into(
            &self.puppet_clips,
            layers,
            time_ms,
            inverse_bind_world,
            vertices,
        )
    }

    pub(crate) fn sample_puppet_animation_vertices_with_clips_and_inverse_bind_into(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let matrices = self.sample_puppet_skin_matrices_with_clips_and_inverse_bind(
            clips,
            layers,
            time_ms,
            inverse_bind_world,
        )?;
        self.sample_puppet_animation_vertices_with_skin_matrices_into(&matrices, vertices)
    }

    pub(crate) fn sample_puppet_skin_matrices_with_clips_and_inverse_bind(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
    ) -> Option<ScenePuppetSkinningMatrices> {
        let skin = self.skin.as_ref()?;
        if inverse_bind_world.len() != skin.bones.len() {
            return None;
        }
        let (skin, local_pose) =
            self.sample_puppet_local_pose_with_clips(clips, layers, time_ms)?;
        let pose_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            local_pose.iter().map(|transform| transform.matrix()),
        )?;
        let skin_matrices = pose_world
            .iter()
            .zip(inverse_bind_world)
            .map(|(pose, inverse_bind)| scene_puppet_matrix_mul(*pose, *inverse_bind))
            .collect::<Vec<_>>();
        let bone_opacities = local_pose
            .iter()
            .map(|pose| pose.opacity.clamp(0.0, 1.0))
            .collect::<Vec<_>>();
        Some(ScenePuppetSkinningMatrices {
            skin_matrices,
            bone_opacities,
        })
    }

    pub(crate) fn sample_puppet_animation_vertices_with_skin_matrices_into(
        &self,
        matrices: &ScenePuppetSkinningMatrices,
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let skin = self.skin.as_ref()?;
        if skin.vertices.len() != self.vertices.len()
            || matrices.skin_matrices.len() != skin.bones.len()
            || matrices.bone_opacities.len() != skin.bones.len()
        {
            return None;
        }

        vertices.clear();
        vertices.reserve(self.vertices.len().saturating_sub(vertices.capacity()));
        for (vertex, skin_vertex) in self.vertices.iter().zip(&skin.vertices) {
            let mut x = 0.0;
            let mut y = 0.0;
            let mut z = 0.0;
            let mut total_weight = 0.0;
            for slot in 0..4 {
                let weight = skin_vertex.weights[slot];
                if !weight.is_finite() || weight <= f64::EPSILON {
                    continue;
                }
                let bone_index = skin_vertex.bone_indices[slot];
                let point = scene_puppet_transform_point_3d(
                    matrices.skin_matrices[bone_index],
                    vertex.x,
                    vertex.y,
                    0.0,
                );
                x += point[0] * weight;
                y += point[1] * weight;
                z += point[2] * weight;
                total_weight += weight;
            }
            let (sampled_x, sampled_y) = if total_weight > f64::EPSILON {
                (x / total_weight, y / total_weight)
            } else {
                (vertex.x, vertex.y)
            };
            let _ = z;
            vertices.push(SceneMeshVertex {
                x: sampled_x,
                y: sampled_y,
                u: vertex.u,
                v: vertex.v,
                opacity: if total_weight > f64::EPSILON {
                    (0..4)
                        .filter_map(|slot| {
                            let weight = skin_vertex.weights[slot];
                            (weight.is_finite() && weight > f64::EPSILON).then(|| {
                                let bone_index = skin_vertex.bone_indices[slot];
                                matrices
                                    .bone_opacities
                                    .get(bone_index)
                                    .map(|opacity| opacity * weight)
                            })?
                        })
                        .sum::<f64>()
                        / total_weight
                } else {
                    vertex.opacity
                },
            });
        }

        Some(())
    }

    fn puppet_animation_frame_debug(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Vec<ScenePuppetAnimationFrameDebug> {
        let Some(skin) = self.skin.as_ref() else {
            return Vec::new();
        };
        if skin.bones.is_empty() {
            return Vec::new();
        }
        let mut frames = Vec::new();
        for layer in layers {
            if !layer.visible || layer.blend <= 0.0 {
                continue;
            }
            let Some(clip) = self
                .puppet_clips
                .iter()
                .find(|clip| clip.id == layer.clip_id)
            else {
                continue;
            };
            let Some(timing) = clip.sample_timing(layer, time_ms) else {
                continue;
            };
            frames.push(ScenePuppetAnimationFrameDebug {
                clip_id: clip.id,
                clip_name: clip.name.clone(),
                layer_name: layer.name.clone(),
                fps: clip.fps,
                frame_count: clip.frame_count,
                looping: clip.looping,
                rate: layer.rate,
                initial_phase: layer.initial_phase,
                blend: layer.blend,
                additive: layer.additive,
                lock_transforms: layer.lock_transforms,
                frame: timing.frame,
                frame0: timing.frame0,
                frame1: timing.frame1,
                mix: timing.mix,
            });
        }
        frames
    }

    pub(crate) fn sample_puppet_attachment_poses(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<BTreeMap<String, ScenePuppetAttachmentPose>> {
        self.sample_puppet_attachment_poses_with_clips(&self.puppet_clips, layers, time_ms)
    }

    pub(crate) fn sample_puppet_attachment_poses_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<BTreeMap<String, ScenePuppetAttachmentPose>> {
        let (skin, _local_pose, _bind_world, pose_world) =
            self.sample_puppet_pose_world_with_clips(clips, layers, time_ms)?;
        if skin.attachments.is_empty() {
            return None;
        }
        let mut poses = BTreeMap::new();
        for attachment in &skin.attachments {
            let bone_index = attachment.bone_index;
            let pose_point = scene_puppet_transform_point_3d(
                *pose_world.get(bone_index)?,
                attachment.local_position[0],
                attachment.local_position[1],
                attachment.local_position[2],
            );
            let pose_angle = scene_puppet_matrix_rotation_z(*pose_world.get(bone_index)?)?;
            poses.insert(
                attachment.name.clone(),
                ScenePuppetAttachmentPose {
                    x: pose_point[0],
                    y: pose_point[1],
                    rotation_deg: pose_angle.to_degrees(),
                },
            );
        }
        (!poses.is_empty()).then_some(poses)
    }

    fn sample_puppet_pose_world_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<(
        &SceneMeshSkin,
        Vec<ScenePuppetTransform>,
        Vec<[f64; 16]>,
        Vec<[f64; 16]>,
    )> {
        let (skin, local_pose) =
            self.sample_puppet_local_pose_with_clips(clips, layers, time_ms)?;
        let bind_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            skin.bones.iter().map(|bone| bone.bind.matrix()),
        )?;
        let pose_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            local_pose.iter().map(|transform| transform.matrix()),
        )?;
        Some((skin, local_pose, bind_world, pose_world))
    }

    fn sample_puppet_local_pose_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<(&SceneMeshSkin, Vec<ScenePuppetTransform>)> {
        let skin = self.skin.as_ref()?;
        let local_pose = scene_puppet_local_pose_for_skin(skin, clips, layers, time_ms, true)?;
        Some((skin, local_pose))
    }
}

fn scene_puppet_local_pose_for_skin(
    skin: &SceneMeshSkin,
    clips: &[ScenePuppetAnimationClip],
    layers: &[ScenePuppetAnimationLayer],
    time_ms: u64,
    require_active_layer: bool,
) -> Option<Vec<ScenePuppetTransform>> {
    if skin.bones.is_empty() {
        return None;
    }
    let mut local_pose = skin.bones.iter().map(|bone| bone.bind).collect::<Vec<_>>();
    let mut has_layer = false;
    for layer in layers {
        if !layer.visible || layer.blend <= 0.0 {
            continue;
        }
        let clip = clips.iter().find(|clip| clip.id == layer.clip_id)?;
        let sampled = clip.sample(layer, time_ms, skin.bones.len())?;
        let blend = layer.blend.clamp(0.0, 1.0);
        for (bone_index, transform) in sampled.iter().enumerate() {
            let bind = skin.bones.get(bone_index)?.bind;
            if layer.lock_transforms {
                local_pose[bone_index] = local_pose[bone_index].blend_opacity_only(
                    bind,
                    *transform,
                    blend,
                    layer.additive,
                );
            } else if layer.additive {
                local_pose[bone_index] =
                    local_pose[bone_index].additive_blend(bind, *transform, blend);
            } else {
                local_pose[bone_index] = local_pose[bone_index].lerp(*transform, blend);
            }
        }
        has_layer = true;
    }
    (!require_active_layer || has_layer).then_some(local_pose)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshPuppetClippingRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    pub mask: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_resource: Option<String>,
    #[serde(default)]
    pub duration_frames: u32,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub bones: Vec<usize>,
    #[serde(default)]
    pub frame_keys: Vec<u32>,
}

impl SceneMeshPuppetClippingRecord {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        validate_required_text("scene mesh puppet clipping mask", &self.mask)?;
        validate_optional_text("scene mesh puppet clipping source name", &self.source_name)?;
        validate_optional_text(
            "scene mesh puppet clipping mask resource",
            &self.mask_resource,
        )?;
        if self.bones.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh puppet clipping record {index} must reference at least one bone"
            )));
        }
        for bone in &self.bones {
            if *bone >= bone_count {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh puppet clipping record {index} bone {bone} is outside the bone array"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshPuppetClippingActiveSource {
    pub source_name: String,
    pub source_id: u64,
    pub scalar_bits: u32,
    pub source_scale: u32,
    pub flags: u32,
    pub transform_index: u32,
    pub parameter0: f32,
    pub parameter1: f32,
}

impl SceneMeshPuppetClippingActiveSource {
    fn validate(&self, node_id: &str, index: usize) -> Result<(), SceneError> {
        validate_required_text(
            "scene mesh puppet clipping active source name",
            &self.source_name,
        )?;
        if !self.parameter0.is_finite() || !self.parameter1.is_finite() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh puppet clipping active source {index} parameters must be finite"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshVertex {
    pub x: f64,
    pub y: f64,
    pub u: f64,
    pub v: f64,
    #[serde(
        default = "default_opacity",
        skip_serializing_if = "is_default_opacity"
    )]
    pub opacity: f64,
}

impl Default for SceneMeshVertex {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            u: 0.0,
            v: 0.0,
            opacity: 1.0,
        }
    }
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
        validate_opacity(
            self.opacity,
            &format!("node {node_id:?} mesh vertex {index}"),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkin {
    #[serde(default)]
    pub bones: Vec<SceneMeshSkinBone>,
    #[serde(default)]
    pub vertices: Vec<SceneMeshSkinVertex>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<SceneMeshSkinAttachment>,
}

impl SceneMeshSkin {
    fn validate(&self, node_id: &str, vertex_count: usize) -> Result<(), SceneError> {
        if self.bones.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin must contain at least one bone"
            )));
        }
        if self.vertices.len() != vertex_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin vertex count {} does not match mesh vertex count {vertex_count}",
                self.vertices.len()
            )));
        }
        for (index, bone) in self.bones.iter().enumerate() {
            bone.validate(node_id, index, self.bones.len())?;
        }
        for (index, vertex) in self.vertices.iter().enumerate() {
            vertex.validate(node_id, index, self.bones.len())?;
        }
        for (index, attachment) in self.attachments.iter().enumerate() {
            attachment.validate(node_id, index, self.bones.len())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinAttachment {
    pub name: String,
    pub bone_index: usize,
    #[serde(default)]
    pub local_position: [f64; 3],
    #[serde(default)]
    pub bind_position: [f64; 3],
}

impl SceneMeshSkinAttachment {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        validate_required_text("scene mesh skin attachment name", &self.name)?;
        if self.bone_index >= bone_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin attachment {index} bone index {} is outside the bone array",
                self.bone_index
            )));
        }
        for (field, values) in [
            ("local_position", self.local_position),
            ("bind_position", self.bind_position),
        ] {
            for (component, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SceneError::invalid(format!(
                        "scene node {node_id:?} mesh skin attachment {index} {field}[{component}] must be finite"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScenePuppetAttachmentPose {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) rotation_deg: f64,
}

impl ScenePuppetAttachmentPose {
    fn transform(self) -> SceneTransform {
        SceneTransform {
            x: self.x,
            y: self.y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: self.rotation_deg,
            anchor_x: 0.5,
            anchor_y: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScenePuppetSkinningMatrices {
    pub(crate) skin_matrices: Vec<[f64; 16]>,
    pub(crate) bone_opacities: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinBone {
    #[serde(default)]
    pub parent: Option<usize>,
    #[serde(default)]
    pub bind: ScenePuppetTransform,
}

impl SceneMeshSkinBone {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        if let Some(parent) = self.parent
            && parent >= bone_count
        {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin bone {index} parent {parent} is outside the bone array"
            )));
        }
        self.bind.validate(node_id, "mesh skin bind transform")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinVertex {
    #[serde(default)]
    pub bone_indices: [usize; 4],
    #[serde(default)]
    pub weights: [f64; 4],
}

impl SceneMeshSkinVertex {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        for (slot, bone_index) in self.bone_indices.iter().enumerate() {
            if *bone_index >= bone_count {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh skin vertex {index} bone slot {slot} index {bone_index} is outside the bone array"
                )));
            }
        }
        for (slot, weight) in self.weights.iter().enumerate() {
            if !weight.is_finite() || *weight < 0.0 {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh skin vertex {index} weight slot {slot} must be finite and non-negative"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationClip {
    pub id: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub frame_count: u32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub bones: Vec<ScenePuppetAnimationBone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePuppetAnimationFrameDebug {
    pub clip_id: u32,
    pub clip_name: Option<String>,
    pub layer_name: Option<String>,
    pub fps: f64,
    pub frame_count: u32,
    pub looping: bool,
    pub rate: f64,
    pub initial_phase: f64,
    pub blend: f64,
    pub additive: bool,
    pub lock_transforms: bool,
    pub frame: f64,
    pub frame0: usize,
    pub frame1: usize,
    pub mix: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScenePuppetAnimationFrameTiming {
    frame: f64,
    frame0: usize,
    frame1: usize,
    mix: f64,
}
