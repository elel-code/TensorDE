
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
                scene_effect_adjustment_at(&self.effects, width, height, time_ms);
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
                push_builtin_effect_snapshot_layers(time_ms, &self.effects, &layer, output);
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
                scene_effect_adjustment_at(&self.effects, width, height, time_ms);
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
                scene_effect_adjustment_at(&self.effects, width, height, time_ms);
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
                push_builtin_effect_snapshot_layers(time_ms, &self.effects, &layer, output);
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

}

include!("node_layers/particle_snapshot.rs");
include!("node_layers/effects_and_transform.rs");
