impl SceneNode {
    // Snapshot expansion receives the already-resolved scene context explicitly.
    #[allow(clippy::too_many_arguments)]
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
        let blend_mode = scene_blend_mode_from_properties(&self.properties);
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
                texture_slots: source
                    .as_ref()
                    .map(|source| {
                        vec![SceneTextureSlot {
                            slot: 0,
                            source: source.clone(),
                            width: None,
                            height: None,
                        }]
                    })
                    .unwrap_or_default(),
                alpha_texture_slot: None,
                alpha_texture_mode: SceneAlphaTextureMode::Multiply,
                image_effect_passes: Vec::new(),
                composite_key: None,
                texture_region,
                effect_motion: SceneNativeEffectMotion::default(),
                blend_mode,
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

    #[allow(clippy::too_many_arguments)]
    fn push_particle_sampled_image_snapshot_layers(
        &self,
        time_ms: u64,
        transform: SceneTransform,
        opacity: f64,
        resources: &[SceneResource],
        build_index: &SceneSnapshotSampledImageBuildIndex,
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
        let source_resource = self
            .resource
            .as_deref()
            .and_then(|resource| build_index.resource(resources, resource));
        let Some(source_resource) = source_resource else {
            return true;
        };
        let texture_slots = vec![SceneTextureSlot {
            slot: 0,
            source: source_resource.source.clone(),
            width: source_resource.width,
            height: source_resource.height,
        }];
        let texture_region = scene_texture_region_from_properties(&self.properties, time_ms);
        let blend_mode = scene_blend_mode_from_properties(&self.properties);
        let tint = scene_tint_from_color(Some(&settings.color));
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
                id: format!("{}#particle-{index}", self.id),
                has_source: true,
                texture_slots: texture_slots.clone(),
                alpha_texture_slot: None,
                alpha_texture_mode: SceneAlphaTextureMode::Multiply,
                image_effect_passes: Vec::new(),
                composite_key: None,
                texture_region,
                width: Some(settings.particle_width),
                height: Some(settings.particle_height),
                mesh: None,
                effect_motion: SceneNativeEffectMotion::default(),
                blend_mode,
                tint,
                fit: self.fit,
                opacity: layer_opacity.clamp(0.0, 1.0),
                transform: particle_transform,
                puppet_animation_frames: Vec::new(),
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
        let blend_mode = scene_blend_mode_from_properties(&self.properties);
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
                texture_slots: Vec::new(),
                alpha_texture_slot: None,
                alpha_texture_mode: SceneAlphaTextureMode::Multiply,
                image_effect_passes: Vec::new(),
                composite_key: None,
                texture_region: None,
                effect_motion: SceneNativeEffectMotion::default(),
                blend_mode,
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

    fn scene_layer_composite_key(
        &self,
        source_resource: Option<&SceneResource>,
    ) -> Option<SceneLayerCompositeKey> {
        let provenance = self.provenance.as_ref()?;
        let attachment = self
            .puppet_attachment
            .as_ref()
            .or(provenance.attachment.as_ref())?;
        let original_path = provenance.original_path.as_ref()?;
        Some(SceneLayerCompositeKey {
            parent_source_id: provenance.parent_id.clone(),
            puppet_attachment: attachment.clone(),
            original_path: original_path.clone(),
            base_source: source_resource?.source.clone(),
        })
    }

    fn snapshot_mesh_at(&self, time_ms: u64) -> Option<Arc<SceneMesh>> {
        let mesh = self.mesh.as_ref()?;
        if self.puppet_animation_layers.is_empty() {
            return Some(mesh.clone());
        }
        mesh.sample_puppet_animation(&self.puppet_animation_layers, time_ms)
            .map(Arc::new)
            .or_else(|| Some(mesh.clone()))
    }

    fn snapshot_puppet_animation_frames_at(
        &self,
        time_ms: u64,
    ) -> Vec<ScenePuppetAnimationFrameDebug> {
        let Some(mesh) = self.mesh.as_ref() else {
            return Vec::new();
        };
        if self.puppet_animation_layers.is_empty() {
            return Vec::new();
        }
        mesh.puppet_animation_frame_debug(&self.puppet_animation_layers, time_ms)
    }

    fn snapshot_puppet_attachment_poses(
        &self,
        time_ms: u64,
    ) -> Option<BTreeMap<String, ScenePuppetAttachmentPose>> {
        self.mesh
            .as_ref()?
            .sample_puppet_attachment_poses(&self.puppet_animation_layers, time_ms)
    }

    fn apply_puppet_attachment_pose(
        &self,
        transform: SceneTransform,
        parent_puppet_attachment_poses: Option<&BTreeMap<String, ScenePuppetAttachmentPose>>,
    ) -> SceneTransform {
        let Some(attachment) = self.puppet_attachment.as_deref() else {
            return transform;
        };
        let Some(pose) = parent_puppet_attachment_poses.and_then(|poses| poses.get(attachment))
        else {
            return transform;
        };
        pose.transform().compose(transform)
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
