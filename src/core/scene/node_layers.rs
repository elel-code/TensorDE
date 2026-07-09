
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
        if let Some(attachment) = &self.puppet_attachment {
            validate_required_text("scene node puppet_attachment", attachment)?;
        }
        if !self.puppet_animation_layers.is_empty() {
            let Some(mesh) = &self.mesh else {
                return Err(SceneError::invalid(format!(
                    "scene node {:?} has puppet animation layers without a mesh",
                    self.id
                )));
            };
            let clip_ids = mesh
                .puppet_clips
                .iter()
                .map(|clip| clip.id)
                .collect::<BTreeSet<_>>();
            if mesh.skin.is_none() || clip_ids.is_empty() {
                return Err(SceneError::invalid(format!(
                    "scene node {:?} has puppet animation layers without mesh skin/clips",
                    self.id
                )));
            }
            for layer in &self.puppet_animation_layers {
                layer.validate(&self.id, &clip_ids)?;
            }
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

    fn runtime_visibility_matches(
        &self,
        resolve_property: &impl Fn(&str) -> Option<f64>,
        resolve_text_property: &impl Fn(&str) -> Option<String>,
    ) -> bool {
        let Some(condition) = self
            .properties
            .get("visibility_condition")
            .and_then(Value::as_object)
        else {
            return true;
        };
        if condition
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime != "wallpaper-engine-user-condition")
        {
            return true;
        }
        let authored_visible = condition
            .get("authored_value")
            .and_then(scene_runtime_visibility_value_bool)
            .unwrap_or(true);
        let Some(property) = condition
            .get("property")
            .and_then(scene_runtime_visibility_value_string)
        else {
            return condition
                .get("default_visible")
                .and_then(scene_runtime_visibility_value_bool)
                .unwrap_or(true);
        };
        let Some(expected) = condition.get("condition") else {
            return condition
                .get("default_visible")
                .and_then(scene_runtime_visibility_value_bool)
                .unwrap_or(authored_visible);
        };
        let actual_number = resolve_property(&property);
        let actual_text = resolve_text_property(&property);
        if actual_number.is_none() && actual_text.is_none() {
            return condition
                .get("default_visible")
                .and_then(scene_runtime_visibility_value_bool)
                .unwrap_or(authored_visible);
        }
        scene_runtime_visibility_condition_matches(expected, actual_number, actual_text.as_deref())
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
        parent_puppet_attachment_poses: Option<&BTreeMap<String, ScenePuppetAttachmentPose>>,
        options: SceneSnapshotBuildOptions,
        output: &mut Vec<SceneSnapshotLayer>,
    ) {
        if !self.visible
            || !self.runtime_visibility_matches(resolve_property, resolve_text_property)
        {
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

        transform = self.apply_puppet_attachment_pose(transform, parent_puppet_attachment_poses);
        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        let puppet_attachment_poses = self.snapshot_puppet_attachment_poses(time_ms);
        let child_puppet_attachment_poses = puppet_attachment_poses.as_ref();
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
                    child_puppet_attachment_poses,
                    options,
                    output,
                );
            }
            return;
        }

        if self.kind != SceneNodeKind::Group {
            let texture_region = scene_texture_region_from_properties(&self.properties, time_ms);
            let blend_mode = scene_blend_mode_from_properties(&self.properties);
            let text = scene_text_from_properties(&self.properties, resolve_text_property)
                .or_else(|| self.text.clone());
            let color = scene_color_from_properties(
                &self.properties,
                "color_binding",
                resolve_text_property,
            )
            .or_else(|| self.color.clone());
            let stroke_color = scene_color_from_properties(
                &self.properties,
                "stroke_color_binding",
                resolve_text_property,
            )
            .or_else(|| self.stroke_color.clone());
            let audio = scene_audio_cues_for_snapshot(&self.audio, resolve_property);
            let layer_effect =
                scene_native_effect_adjustment_at(&self.effects, width, height, time_ms);
            let layer_transform = layer_effect.apply_transform(transform);
            let layer_opacity = layer_effect.apply_opacity(opacity);
            let mesh = self.snapshot_mesh_at(time_ms);
            let source_resource = self
                .resource
                .as_deref()
                .and_then(|resource| resources.get(resource))
                .copied();
            let (texture_slots, alpha_texture_slot, alpha_texture_mode) =
                scene_texture_slots_for_node(source_resource, &self.effects, |resource_id| {
                    resources.get(resource_id).copied()
                });
            let image_effect_passes =
                scene_image_effect_passes_for_node(&self.effects, |resource_id| {
                    resources.get(resource_id).copied()
                });
            let composite_key = self.scene_layer_composite_key(source_resource);
            let layer = SceneSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: source_resource.map(|resource| resource.source.clone()),
                texture_slots,
                alpha_texture_slot,
                alpha_texture_mode,
                image_effect_passes,
                composite_key,
                texture_region,
                effect_motion: layer_effect.motion,
                blend_mode,
                audio,
                color,
                stroke_color,
                stroke_width: self.stroke_width,
                corner_radius,
                width,
                height,
                mesh,
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
                opacity: layer_opacity,
                transform: layer_transform,
            };
            if scene_snapshot_layer_intersects_visibility(&layer, visibility) {
                push_native_effect_snapshot_layers(time_ms, &self.effects, &layer, output);
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
                child_puppet_attachment_poses,
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
        resources: &[SceneResource],
        timelines: &[SceneTimeline],
        property_bindings: &[ScenePropertyBinding],
        build_index: &SceneSnapshotSampledImageBuildIndex,
        resolve_property: &impl Fn(&str) -> Option<f64>,
        resolve_text_property: &impl Fn(&str) -> Option<String>,
        visibility: Option<SceneSnapshotVisibility>,
        parent_puppet_attachment_poses: Option<&BTreeMap<String, ScenePuppetAttachmentPose>>,
        output: &mut Vec<SceneSnapshotSampledImageLayer>,
    ) {
        if !self.visible
            || !self.runtime_visibility_matches(resolve_property, resolve_text_property)
        {
            return;
        }
        let mut transform = self.transform;
        let mut opacity = self.opacity;
        let mut width = self.width;
        let mut height = self.height;
        let mut corner_radius = self.corner_radius;
        for &timeline_index in build_index.timeline_indices_for_node(&self.id) {
            let Some(timeline) = timelines.get(timeline_index) else {
                continue;
            };
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
        for &binding_index in build_index.global_property_binding_indices() {
            let Some(binding) = property_bindings.get(binding_index) else {
                continue;
            };
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
        for &binding_index in build_index.property_binding_indices_for_node(&self.id) {
            let Some(binding) = property_bindings.get(binding_index) else {
                continue;
            };
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

        transform = self.apply_puppet_attachment_pose(transform, parent_puppet_attachment_poses);
        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        let puppet_attachment_poses = self.snapshot_puppet_attachment_poses(time_ms);
        let child_puppet_attachment_poses = puppet_attachment_poses.as_ref();
        if self.kind == SceneNodeKind::ParticleEmitter
            && self.push_particle_sampled_image_snapshot_layers(
                time_ms,
                transform,
                opacity,
                resources,
                build_index,
                visibility,
                output,
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
                    build_index,
                    resolve_property,
                    resolve_text_property,
                    visibility,
                    child_puppet_attachment_poses,
                    output,
                );
            }
            return;
        }

        if self.kind == SceneNodeKind::Image {
            let source_resource = self
                .resource
                .as_deref()
                .and_then(|resource| build_index.resource(resources, resource));
            let (texture_slots, alpha_texture_slot, alpha_texture_mode) =
                scene_texture_slots_for_node(source_resource, &self.effects, |resource_id| {
                    build_index.resource(resources, resource_id)
                });
            let image_effect_passes =
                scene_image_effect_passes_for_node(&self.effects, |resource_id| {
                    build_index.resource(resources, resource_id)
                });
            let composite_key = self.scene_layer_composite_key(source_resource);
            let blend_mode = scene_blend_mode_from_properties(&self.properties);
            let color = scene_color_from_properties(
                &self.properties,
                "color_binding",
                resolve_text_property,
            )
            .or_else(|| self.color.clone());
            let tint = scene_tint_from_color(color.as_deref());
            let layer_effect =
                scene_native_effect_adjustment_at(&self.effects, width, height, time_ms);
            let layer_transform = layer_effect.apply_transform(transform);
            let layer_opacity = layer_effect.apply_opacity(opacity);
            let puppet_animation_frames = self.snapshot_puppet_animation_frames_at(time_ms);
            let mesh = self.snapshot_mesh_at(time_ms);
            let layer = SceneSnapshotSampledImageLayer {
                id: self.id.clone(),
                has_source: source_resource.is_some(),
                texture_slots,
                alpha_texture_slot,
                alpha_texture_mode,
                image_effect_passes,
                composite_key,
                texture_region: scene_texture_region_from_properties(&self.properties, time_ms),
                width,
                height,
                mesh,
                effect_motion: layer_effect.motion,
                blend_mode,
                tint,
                fit: self.fit,
                opacity: layer_opacity,
                transform: layer_transform,
                puppet_animation_frames,
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
                build_index,
                resolve_property,
                resolve_text_property,
                visibility,
                child_puppet_attachment_poses,
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
        parent_puppet_attachment_poses: Option<&BTreeMap<String, ScenePuppetAttachmentPose>>,
        output: &mut Vec<SceneSnapshotLayer>,
    ) {
        if !self.visible
            || !self.runtime_visibility_matches(resolve_property, resolve_text_property)
        {
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

        transform = self.apply_puppet_attachment_pose(transform, parent_puppet_attachment_poses);
        if let Some(depth) = self.parallax_depth
            && depth.is_finite()
        {
            transform.x += parallax.x * depth;
            transform.y += parallax.y * depth;
        }
        let transform = parent_transform.compose(transform);
        let opacity = (parent_opacity * opacity).clamp(0.0, 1.0);
        let puppet_attachment_poses = self.snapshot_puppet_attachment_poses(time_ms);
        let child_puppet_attachment_poses = puppet_attachment_poses.as_ref();
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
                    child_puppet_attachment_poses,
                    output,
                );
            }
            return;
        }

        if self.subtree_self_has_solid_visual_geometry() {
            let blend_mode = scene_blend_mode_from_properties(&self.properties);
            let text = scene_text_from_properties(&self.properties, resolve_text_property)
                .or_else(|| self.text.clone());
            let color = scene_color_from_properties(
                &self.properties,
                "color_binding",
                resolve_text_property,
            )
            .or_else(|| self.color.clone());
            let stroke_color = scene_color_from_properties(
                &self.properties,
                "stroke_color_binding",
                resolve_text_property,
            )
            .or_else(|| self.stroke_color.clone());
            let layer_effect =
                scene_native_effect_adjustment_at(&self.effects, width, height, time_ms);
            let layer_transform = layer_effect.apply_transform(transform);
            let layer_opacity = layer_effect.apply_opacity(opacity);
            let layer = SceneSnapshotLayer {
                id: self.id.clone(),
                kind: self.kind,
                source: None,
                texture_slots: Vec::new(),
                alpha_texture_slot: None,
                alpha_texture_mode: SceneAlphaTextureMode::Multiply,
                image_effect_passes: Vec::new(),
                composite_key: None,
                texture_region: None,
                effect_motion: layer_effect.motion,
                blend_mode,
                audio: Vec::new(),
                color,
                stroke_color,
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
                opacity: layer_opacity,
                transform: layer_transform,
            };
            if scene_snapshot_layer_intersects_visibility(&layer, visibility) {
                push_native_effect_snapshot_layers(time_ms, &self.effects, &layer, output);
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
                child_puppet_attachment_poses,
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
    pub fbos: Vec<SceneEffectFbo>,
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
        for fbo in &self.fbos {
            fbo.validate(node_id, &self.file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectFbo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default = "scene_effect_fbo_default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub unique: bool,
}

fn scene_effect_fbo_default_scale() -> f64 {
    1.0
}

impl SceneEffectFbo {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        validate_required_text(
            &format!("scene node {node_id:?} effect {effect_file:?} fbo name"),
            &self.name,
        )?;
        if let Some(format) = &self.format {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} fbo format"),
                format,
            )?;
        }
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} effect {effect_file:?} fbo {:?} scale must be positive",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectPass {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binds: BTreeMap<u32, String>,
    #[serde(default)]
    pub shader: Option<String>,
    #[serde(default)]
    pub blending: Option<String>,
    #[serde(default)]
    pub depthtest: Option<String>,
    #[serde(default)]
    pub depthwrite: Option<String>,
    #[serde(default)]
    pub cullmode: Option<String>,
    #[serde(default)]
    pub alphawriting: Option<String>,
    #[serde(default)]
    pub textures: Vec<Option<String>>,
    #[serde(default)]
    pub texture_resources: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_uv_transform: Option<SceneEffectUvTransform>,
    #[serde(default)]
    pub combos: BTreeMap<String, i64>,
    #[serde(default)]
    pub constant_shader_values: BTreeMap<String, Value>,
    #[serde(default)]
    pub user_textures: Option<Value>,
}

impl SceneEffectPass {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("command", self.command.as_deref()),
            ("source", self.source.as_deref()),
            ("target", self.target.as_deref()),
            ("shader", self.shader.as_deref()),
            ("blending", self.blending.as_deref()),
            ("depthtest", self.depthtest.as_deref()),
            ("depthwrite", self.depthwrite.as_deref()),
            ("cullmode", self.cullmode.as_deref()),
            ("alphawriting", self.alphawriting.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} effect {effect_file:?} pass {field} must not be empty"
                )));
            }
        }
        for bind in self.binds.values() {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} pass bind"),
                bind,
            )?;
        }
        for texture in self.textures.iter().flatten() {
            if texture.trim().is_empty() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} effect {effect_file:?} texture reference must not be empty"
                )));
            }
        }
        for texture_resource in self.texture_resources.iter().flatten() {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} texture resource"),
                texture_resource,
            )?;
        }
        if let Some(transform) = self.effect_uv_transform {
            transform.validate(node_id, effect_file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneEffectUvMapping {
    #[default]
    TextureResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectUvExtent {
    pub width: u32,
    pub height: u32,
}

impl SceneEffectUvExtent {
    fn validate(&self, node_id: &str, effect_file: &str, label: &str) -> Result<(), SceneError> {
        if self.width == 0 || self.height == 0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} effect {effect_file:?} {label} extent must be non-zero"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectUvTransform {
    #[serde(default)]
    pub mapping: SceneEffectUvMapping,
    #[serde(default)]
    pub source_slot: u32,
    #[serde(default)]
    pub mask_slot: u32,
    #[serde(default = "default_effect_uv_scale")]
    pub scale: [f64; 2],
    #[serde(default)]
    pub offset: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_extent: Option<SceneEffectUvExtent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_extent: Option<SceneEffectUvExtent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_backing_extent: Option<SceneEffectUvExtent>,
}

impl SceneEffectUvTransform {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        for (field, values) in [("scale", self.scale), ("offset", self.offset)] {
            for (index, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SceneError::invalid(format!(
                        "scene node {node_id:?} effect {effect_file:?} effect UV {field}[{index}] must be finite"
                    )));
                }
            }
        }
        if let Some(extent) = self.input_extent {
            extent.validate(node_id, effect_file, "input")?;
        }
        if let Some(extent) = self.mask_extent {
            extent.validate(node_id, effect_file, "mask")?;
        }
        if let Some(extent) = self.mask_backing_extent {
            extent.validate(node_id, effect_file, "mask backing")?;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneBlendMode {
    #[default]
    Alpha,
    Normal,
    Additive,
    Multiply,
    Screen,
    Max,
    /// Wallpaper Engine colorBlendMode 32: `A * (1 + B*a)` - a background-modulated
    /// brighten that vanishes on dark backgrounds. With a premultiplied source (B*a) this
    /// is the fixed-function equation `src*DST_COLOR + dst*ONE`.
    Modulate,
    /// Wallpaper Engine colorBlendMode 28: HSL Color. This is not a fixed-function
    /// blend; WE evaluates it in a framebuffer-sampling passthrough shader.
    HslColor,
    /// Wallpaper Engine material pass `blending:"alphatocoverage"`: shader alpha
    /// becomes the MSAA coverage mask and fixed-function color blending is disabled.
    AlphaToCoverage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAlphaTextureMode {
    #[default]
    Multiply,
    Inverse,
    Iris,
    Coverage,
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
