use super::*;
use crate::engine::scene::binary::SceneBinaryDocument;

mod fixtures;

use fixtures::*;

#[test]
fn semantic_world_indexes_components_by_scene_object_handle() {
    let storage = SceneStorage::from_document(semantic_document()).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");

    let image = SceneObjectHandle(0);
    let entity = world.entity_for_object(image).expect("image entity");
    let transform = world.transform(image).expect("image transform");
    let visibility = world.visibility(image).expect("image visibility");
    let material = world.material_binding(image).expect("image material");
    let mesh_bindings = world.object_mesh_bindings(image);

    assert_eq!(entity.index(), 0);
    assert_eq!(world.entity_for_we_id(100), Some(entity));
    assert_eq!(transform.origin.x, 10.0);
    assert_eq!(transform.scale.x, 2.0);
    assert!(visibility.visible);
    assert_eq!(visibility.sort_order, 3);
    assert_eq!(material.material, SceneMaterialHandle(0));
    assert_eq!(mesh_bindings.len(), 2);
    assert_eq!(mesh_bindings[0].mesh_index, 0);
    assert_eq!(mesh_bindings[1].mesh_index, 2);
    assert_eq!(mesh_bindings[0].vertex_count, 4);
}

#[test]
fn resolve_frame_keeps_parent_and_child_visual_state_independent() {
    let mut document = semantic_document();
    document.objects[0].color = SceneVec3 {
        x: 0.5,
        y: 0.75,
        z: 1.0,
    };
    document.objects[0].alpha = 0.5;
    document.objects[1].color = SceneVec3 {
        x: 0.2,
        y: 0.4,
        z: 0.8,
    };
    document.objects[1].alpha = 0.4;
    let storage = SceneStorage::from_document(document).expect("storage");
    let frame = SceneSemanticWorld::from_storage(&storage)
        .expect("semantic world")
        .resolve_frame()
        .expect("resolved frame");

    let child = frame.object(SceneObjectHandle(1)).expect("child");
    assert_eq!(
        child.resolved_color,
        SceneVec3 {
            x: 0.2,
            y: 0.4,
            z: 0.8,
        }
    );
    assert!((child.resolved_alpha - 0.4).abs() < f32::EPSILON);
}

#[test]
fn visible_user_property_resolves_before_parent_visibility_and_keeps_effect_state_independent() {
    let mut document = semantic_document();
    document.objects[0].visible = false;
    let schema = document.strings.len() as u32;
    document
        .strings
        .push(r#"{"rain":{"type":"bool","value":false}}"#.to_owned());
    let property = document.strings.len() as u32;
    document.strings.push("rain".to_owned());
    document.project.properties_json = SceneStringId(schema);
    document
        .user_property_bindings
        .push(SceneUserPropertyBindingRecord {
            object: SceneObjectHandle(0),
            property: SceneStringId(property),
            target: SceneUserPropertyTarget::Visible,
            predicate: SceneUserPropertyPredicate::BooleanValue,
        });
    document.effects.push(SceneEffectRecord {
        id: SceneEffectHandle(0),
        resource: SceneResourceId::NONE,
        replacement_key: SceneStringId::NONE,
        pass_start: 0,
        pass_count: 0,
        fbo_start: 0,
        fbo_count: 0,
    });
    document.object_effects.push(SceneObjectEffectRecord {
        object: SceneObjectHandle(0),
        effect: SceneEffectHandle(0),
        name: SceneStringId::NONE,
        instance_id: 12,
        visible: true,
    });
    document.objects[0].effect_start = 0;
    document.objects[0].effect_count = 1;

    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let default_frame = world.resolve_frame().expect("default property frame");

    assert!(
        !default_frame
            .object(SceneObjectHandle(0))
            .unwrap()
            .self_visible
    );
    assert!(
        !default_frame
            .object(SceneObjectHandle(1))
            .unwrap()
            .resolved_visible
    );
    assert!(default_frame.object_effects[0].self_visible);
    assert!(!default_frame.object_effects[0].resolved_visible);

    let overrides = [("rain".to_owned(), serde_json::Value::Bool(true))]
        .into_iter()
        .collect();
    let enabled_frame = world
        .resolve_frame_with_user_properties_at(0.0, &overrides)
        .expect("enabled property frame");
    assert!(
        enabled_frame
            .object(SceneObjectHandle(0))
            .unwrap()
            .self_visible
    );
    assert!(
        enabled_frame
            .object(SceneObjectHandle(1))
            .unwrap()
            .resolved_visible
    );
    assert!(enabled_frame.object_effects[0].resolved_visible);

    let resolver = SemanticFrameResolver::from_world_with_user_properties(&world, &overrides)
        .expect("retained resolver");
    assert_eq!(resolver.dynamic_entity_count(), 2);
}

#[test]
fn combo_visibility_condition_resolves_by_exact_string_equality() {
    let mut document = semantic_document();
    document.objects[0].visible = false;
    let schema = document.strings.len() as u32;
    document.strings.push(
        r#"{"theme":{"type":"combo","value":"1","options":[{"value":"1"},{"value":"2"}]}}"#
            .to_owned(),
    );
    let property = document.strings.len() as u32;
    document.strings.push("theme".to_owned());
    let condition = document.strings.len() as u32;
    document.strings.push("2".to_owned());
    document.project.properties_json = SceneStringId(schema);
    document
        .user_property_bindings
        .push(SceneUserPropertyBindingRecord {
            object: SceneObjectHandle(0),
            property: SceneStringId(property),
            target: SceneUserPropertyTarget::Visible,
            predicate: SceneUserPropertyPredicate::StringEquals(SceneStringId(condition)),
        });

    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let default_frame = world.resolve_frame().expect("default combo frame");
    assert!(
        !default_frame
            .object(SceneObjectHandle(0))
            .unwrap()
            .self_visible
    );
    assert!(
        !default_frame
            .object(SceneObjectHandle(1))
            .unwrap()
            .resolved_visible
    );

    let overrides = [(
        "theme".to_owned(),
        serde_json::Value::String("2".to_owned()),
    )]
    .into_iter()
    .collect();
    let selected_frame = world
        .resolve_frame_with_user_properties_at(0.0, &overrides)
        .expect("selected combo frame");
    assert!(
        selected_frame
            .object(SceneObjectHandle(0))
            .unwrap()
            .self_visible
    );
    assert!(
        selected_frame
            .object(SceneObjectHandle(1))
            .unwrap()
            .resolved_visible
    );
}

#[test]
fn semantic_world_exposes_puppet_bones_and_attachments_without_abi_entities() {
    let storage = SceneStorage::from_document(semantic_document()).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let puppet_object = SceneObjectHandle(1);

    let parent = world.parent(puppet_object).expect("parent component");
    let puppet = world
        .puppet_binding(puppet_object)
        .expect("puppet component");
    let bones = world.puppet_bones(puppet_object).expect("puppet bones");
    let attachments = world
        .puppet_attachments(puppet_object)
        .expect("puppet attachments");
    let inputs = world.render_plan_inputs();

    assert_eq!(parent.parent_we_id, 100);
    assert_eq!(puppet.bone_count, 1);
    assert_eq!(puppet.attachment_count, 1);
    assert_eq!(bones[0].bone_index, 41);
    assert_eq!(bones[0].parent_index, -1);
    assert_eq!(attachments[0].bone_index, 41);
    assert_eq!(storage.string(attachments[0].name), Some("weapon"));
    assert_eq!(inputs.object_count, 2);
    assert_eq!(inputs.visible_object_count, 2);
    assert_eq!(inputs.mesh_binding_count, 3);
    assert_eq!(inputs.effect_binding_count, 0);
    assert_eq!(inputs.puppet_binding_count, 1);
}

#[test]
fn resolve_frame_carries_object_effect_visibility_and_pass_ranges() {
    let mut document = semantic_document();
    document.effects.push(SceneEffectRecord {
        id: SceneEffectHandle(0),
        resource: SceneResourceId::NONE,
        replacement_key: SceneStringId::NONE,
        pass_start: 0,
        pass_count: 2,
        fbo_start: 0,
        fbo_count: 1,
    });
    document.effect_passes = vec![effect_pass(0), effect_pass(1)];
    document.effect_fbos.push(SceneEffectFboRecord {
        name: SceneStringId::NONE,
        format: SceneStringId::NONE,
        scale: 1.0,
    });
    document.object_effects.push(SceneObjectEffectRecord {
        object: SceneObjectHandle(0),
        effect: SceneEffectHandle(0),
        name: SceneStringId::NONE,
        instance_id: 77,
        visible: true,
    });
    document.object_effects.push(SceneObjectEffectRecord {
        object: SceneObjectHandle(1),
        effect: SceneEffectHandle(0),
        name: SceneStringId::NONE,
        instance_id: 78,
        visible: false,
    });
    document.objects[0].effect_start = 0;
    document.objects[0].effect_count = 1;
    document.objects[1].effect_start = 1;
    document.objects[1].effect_count = 1;

    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let image_effects = world.object_effect_bindings(SceneObjectHandle(0));
    let frame = world.resolve_frame().expect("resolved frame");

    assert_eq!(image_effects.len(), 1);
    assert_eq!(image_effects[0].instance_id, 77);
    assert_eq!(frame.object_effects.len(), 2);
    assert_eq!(frame.visible_effect_instance_count, 1);
    assert_eq!(frame.visible_effect_pass_count, 2);
    assert_eq!(frame.visible_effect_fbo_count, 1);
    assert_eq!(frame.object_effects[0].binding_index, 0);
    assert!(frame.object_effects[0].resolved_visible);
    assert_eq!(frame.object_effects[0].pass_start, 0);
    assert_eq!(frame.object_effects[0].pass_count, 2);
    assert_eq!(frame.object_effects[0].fbo_start, 0);
    assert_eq!(frame.object_effects[0].fbo_count, 1);
    assert!(!frame.object_effects[1].resolved_visible);

    let deltas = [
        SceneScriptDelta {
            object: SceneObjectHandle(0),
            target: SceneScriptTarget::EffectVisible,
            selector: 0,
            numeric: [0.0; 4],
            text: None,
        },
        SceneScriptDelta {
            object: SceneObjectHandle(1),
            target: SceneScriptTarget::EffectVisible,
            selector: 1,
            numeric: [1.0, 0.0, 0.0, 0.0],
            text: None,
        },
    ];
    let mutated = world
        .resolve_frame_with_dynamic_values_at(0.0, 0.0, &deltas, &[None; 2])
        .expect("mutated effect frame");
    assert!(!mutated.object_effects[0].resolved_visible);
    assert!(mutated.object_effects[1].resolved_visible);
    assert_eq!(mutated.visible_effect_instance_count, 1);
}

#[test]
fn resolve_frame_outputs_parent_attachment_world_transform() {
    let storage = SceneStorage::from_document(attachment_document()).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let frame = world.resolve_frame().expect("resolved frame");
    let child = frame.object(SceneObjectHandle(1)).expect("child state");
    let link = frame.attachment_links[0];

    assert_eq!(frame.visible_object_count, 2);
    assert_eq!(frame.visible_mesh_binding_count, 2);
    assert_eq!(frame.visible_effect_instance_count, 0);
    assert_eq!(frame.visible_puppet_binding_count, 1);
    assert_eq!(frame.puppet_bone_palettes.len(), 1);
    assert_eq!(frame.puppet_bone_matrices.len(), 1);
    assert_eq!(frame.visible_puppet_bone_matrix_count, 1);
    assert_eq!(child.parent, SceneObjectHandle(0));
    assert!(child.resolved_visible);
    assert_eq!(child.mesh_binding_start, 1);
    assert_eq!(child.mesh_binding_count, 1);
    assert_eq!(link.parent_puppet_index, 0);
    assert_eq!(link.bone_index, 41);
    assert!(link.resolved);
    assert_eq!(frame.puppet_bone_palettes[0].object, SceneObjectHandle(0));
    assert_eq!(frame.puppet_bone_palettes[0].bone_count, 1);
    assert_eq!(frame.puppet_bone_matrices[0].bone_index, 41);
    assert_close(frame.puppet_bone_matrices[0].matrix[12], 0.0);
    assert_close(frame.puppet_bone_matrices[0].matrix[13], 0.0);
    assert_close(child.world_matrix[12], 17.0);
    assert_close(child.world_matrix[13], 30.0);
}

#[test]
fn resolve_frame_samples_puppet_animation_for_skinning_and_attachment() {
    let mut document = attachment_document();
    document
        .strings
        .extend(["blink".to_owned(), "loop".to_owned()]);
    document
        .object_animation_layers
        .push(SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(0),
            animation_id: 475,
            layer_index: 0,
            additive: false,
            autosort: false,
            visible: true,
            playback_rate: 1.0,
            blend_weight: 1.0,
            initial_progress: 0.0,
        });
    document
        .puppet_animation_transform_samples
        .push(animation_sample([1.0, 2.0, 0.0]));
    document
        .puppet_animation_transform_samples
        .push(animation_sample([30.0, 40.0, 0.0]));
    document
        .puppet_animation_opacity_samples
        .extend([1.0, 0.25]);
    document
        .puppet_animation_tracks
        .push(ScenePuppetAnimationTrackRecord {
            clip: 0,
            bone_index: 41,
            track_flags: 0,
            sample_start: 0,
            sample_count: 2,
            opacity_flags: 9,
            opacity_sample_start: 0,
            opacity_sample_count: 2,
        });
    document
        .puppet_animation_clips
        .push(ScenePuppetAnimationClipRecord {
            puppet: 0,
            clip_id: 475,
            flags: 0,
            name: SceneStringId(2),
            playback: SceneStringId(3),
            fps: 30.0,
            frame_count: 1,
            frame_metadata: 0,
            track_start: 0,
            track_count: 1,
        });
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");

    let frame = world
        .resolve_frame_at(1.0 / 60.0)
        .expect("resolved animated frame");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("semantic resolver");
    assert_eq!(resolver.dynamic_entity_count(), 1);
    assert_eq!(
        resolver
            .resolve_frame_with_events_at(
                &world,
                1.0 / 60.0,
                1.0 / 60.0,
                &crate::engine::scene::SceneFrameEvents::default(),
            )
            .expect("incrementally resolved animated frame"),
        &frame
    );
    let child = frame.object(SceneObjectHandle(1)).expect("child state");

    assert_close(frame.puppet_bone_matrices[0].matrix[12], 15.5);
    assert_close(frame.puppet_bone_matrices[0].matrix[13], 21.0);
    assert_close(frame.puppet_bone_matrices[0].alpha, 0.625);
    assert_close(child.world_matrix[12], 32.5);
    assert_close(child.world_matrix[13], 51.0);
    let graph = crate::engine::scene::RenderingServer::new(&storage)
        .rendering_device_graph_plan_at(1.0 / 60.0);
    assert_close(graph.puppet_bone_matrices[0].alpha, 0.625);
}

#[test]
fn resolve_frame_applies_additive_puppet_delta_from_bind_pose() {
    let mut document = attachment_document();
    document
        .strings
        .extend(["pose-delta".to_owned(), "single".to_owned()]);
    document.puppet_bones[0].local_bind_matrix = translated_attachment_matrix(10.0, 20.0);
    document
        .object_animation_layers
        .push(SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(0),
            animation_id: 549,
            layer_index: 0,
            additive: true,
            autosort: false,
            visible: true,
            playback_rate: 1.0,
            blend_weight: 1.0,
            initial_progress: 0.0,
        });
    document
        .puppet_animation_transform_samples
        .push(animation_sample([13.0, 24.0, 0.0]));
    document
        .puppet_animation_tracks
        .push(ScenePuppetAnimationTrackRecord {
            clip: 0,
            bone_index: 41,
            track_flags: 0,
            sample_start: 0,
            sample_count: 1,
            opacity_flags: 0,
            opacity_sample_start: 0,
            opacity_sample_count: 0,
        });
    document
        .puppet_animation_clips
        .push(ScenePuppetAnimationClipRecord {
            puppet: 0,
            clip_id: 549,
            flags: 0,
            name: SceneStringId(2),
            playback: SceneStringId(3),
            fps: 30.0,
            frame_count: 1,
            frame_metadata: 0,
            track_start: 0,
            track_count: 1,
        });
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let frame = world.resolve_frame().expect("resolved additive frame");
    let child = frame.object(SceneObjectHandle(1)).expect("child state");

    assert_close(frame.puppet_bone_matrices[0].matrix[12], 3.0);
    assert_close(frame.puppet_bone_matrices[0].matrix[13], 4.0);
    assert_close(child.world_matrix[12], 30.0);
    assert_close(child.world_matrix[13], 54.0);
}

#[test]
fn resolve_frame_rejects_parent_cycles_before_render_planning() {
    let storage = SceneStorage::from_document(parent_cycle_document()).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let err = world.resolve_frame().expect_err("parent cycle");

    assert!(matches!(
        err,
        SceneSemanticWorldError::ParentCycle {
            object: SceneObjectHandle(0) | SceneObjectHandle(1)
        }
    ));
}

#[test]
fn rquickjs_audio_module_applies_uniform_object_scale_before_transform_propagation() {
    let mut object = image_object();
    object.material = SceneMaterialHandle(INVALID_MATERIAL_ID);
    let document = SceneBinaryDocument {
        strings: vec![
            "const audio = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_32); export function update(value) { const scale = 1 + audio.average[0]; value.x = scale; value.y = scale; value.z = scale; return value; }".to_owned(),
            "{}".to_owned(),
        ],
        objects: vec![object],
        script_programs: vec![SceneScriptProgramRecord {
            object: SceneObjectHandle(0),
            target: SceneScriptTarget::Scale,
            selector: 0,
            source: SceneStringId(0),
            properties_json: SceneStringId(1),
            initial_text: SceneStringId::NONE,
            subscriptions: SceneScriptSubscriptions::FRAME
                .union(SceneScriptSubscriptions::AUDIO),
            initial_numeric: [1.0, 1.0, 1.0, 0.0],
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("semantic resolver");

    assert_eq!(resolver.dynamic_entity_count(), 1);
    let events = crate::engine::scene::SceneFrameEvents {
        audio: crate::engine::scene::SceneAudioState {
            source: crate::engine::scene::SceneAudioSource::Replay,
            spectrum: crate::engine::scene::StereoSpectrum64 {
                left: [1.0; 64],
                right: [1.0; 64],
            },
            ready: true,
            ..crate::engine::scene::SceneAudioState::default()
        },
        ..crate::engine::scene::SceneFrameEvents::default()
    };
    let frame = resolver
        .resolve_frame_with_events_at(&world, 1.0, 0.0, &events)
        .expect("audio-scaled frame");
    let object = frame.object(SceneObjectHandle(0)).expect("scaled object");

    assert_close(object.local_matrix[0], 2.0);
    assert_close(object.local_matrix[5], 2.0);
    assert_close(object.local_matrix[10], 2.0);
}

#[test]
fn audio_script_publishes_a_typed_material_scalar_selector() {
    let mut object = image_object();
    object.material = SceneMaterialHandle(INVALID_MATERIAL_ID);
    let document = SceneBinaryDocument {
        strings: vec![
            "const audio = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_16); let smoothValue = 0; let initialValue; export function init(value) { initialValue = value; } export function update() { const audioDelta = audio.average[0] - smoothValue; smoothValue += audioDelta * Math.min(1, engine.frametime * 2); return initialValue * (smoothValue * 0.38 + 1); }".to_owned(),
            "{}".to_owned(),
            "缺口大小".to_owned(),
            "{\"value\":225}".to_owned(),
        ],
        objects: vec![object],
        material_constants: vec![SceneMaterialConstantRecord {
            name: SceneStringId(2),
            value_json: SceneStringId(3),
        }],
        script_programs: vec![SceneScriptProgramRecord {
            object: SceneObjectHandle(0),
            target: SceneScriptTarget::MaterialScalar,
            selector: 0,
            source: SceneStringId(0),
            properties_json: SceneStringId(1),
            initial_text: SceneStringId::NONE,
            subscriptions: SceneScriptSubscriptions::FRAME
                .union(SceneScriptSubscriptions::AUDIO),
            initial_numeric: [225.0, 0.0, 0.0, 0.0],
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("semantic resolver");
    let initial = resolver
        .resolve_frame_with_events_at(
            &world,
            0.0,
            0.0,
            &crate::engine::scene::SceneFrameEvents::default(),
        )
        .expect("initial scalar frame");
    assert_eq!(initial.material_scalar_values[0].constant_index, 0);
    assert_close(initial.material_scalar_values[0].value, 225.0);

    let audio = crate::engine::scene::SceneFrameEvents {
        audio: crate::engine::scene::SceneAudioState {
            sequence: crate::engine::scene::SceneEventSequence(1),
            source: crate::engine::scene::SceneAudioSource::Replay,
            spectrum: crate::engine::scene::StereoSpectrum64 {
                left: [1.0; 64],
                right: [1.0; 64],
            },
            ready: true,
            ..crate::engine::scene::SceneAudioState::default()
        },
        ..crate::engine::scene::SceneFrameEvents::default()
    };
    let animated = resolver
        .resolve_frame_with_events_at(&world, 1.0, 1.0, &audio)
        .expect("audio scalar frame");
    assert_eq!(animated.material_scalar_values[0].constant_index, 0);
    assert_close(animated.material_scalar_values[0].value, 310.5);
}

#[test]
fn scene_script_delta_updates_transform_before_parent_resolution() {
    let mut object = image_object();
    object.material = SceneMaterialHandle(INVALID_MATERIAL_ID);
    let document = SceneBinaryDocument {
        strings: vec![
            "export function update(value) { value.x = 25 + engine.runtime; return value; }"
                .to_owned(),
            "{}".to_owned(),
        ],
        objects: vec![object],
        script_programs: vec![SceneScriptProgramRecord {
            object: SceneObjectHandle(0),
            target: SceneScriptTarget::Origin,
            selector: 0,
            source: SceneStringId(0),
            properties_json: SceneStringId(1),
            initial_text: SceneStringId::NONE,
            subscriptions: SceneScriptSubscriptions::FRAME,
            initial_numeric: [10.0, 20.0, 0.0, 0.0],
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("semantic resolver");
    let frame = resolver
        .resolve_frame_with_events_at(
            &world,
            2.0,
            0.0,
            &crate::engine::scene::SceneFrameEvents::default(),
        )
        .expect("scripted frame");
    let object = frame.object(SceneObjectHandle(0)).expect("scripted object");

    assert_close(object.local_matrix[12], 27.0);
    assert_close(object.world_matrix[12], 27.0);
}

#[test]
fn cursor_click_requires_a_matching_press_release_hit_target() {
    let mut document = semantic_document();
    document.project.logical_width = 100;
    document.project.logical_height = 100;
    let source = document.strings.len() as u32;
    document.strings.push(
        "let enabled = true; export function cursorClick(event) { enabled = !enabled; } export function update(value) { return enabled; }"
            .to_owned(),
    );
    let properties = document.strings.len() as u32;
    document.strings.push("{}".to_owned());
    document.script_programs.push(SceneScriptProgramRecord {
        object: SceneObjectHandle(0),
        target: SceneScriptTarget::Visible,
        selector: 0,
        source: SceneStringId(source),
        properties_json: SceneStringId(properties),
        initial_text: SceneStringId::NONE,
        subscriptions: SceneScriptSubscriptions::FRAME
            .union(SceneScriptSubscriptions::POINTER_CLICK),
        initial_numeric: [1.0, 0.0, 0.0, 0.0],
    });
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("semantic resolver");
    let pointer_event = |pressed| crate::engine::scene::ScenePointerEvent {
        source: crate::engine::scene::ScenePointerSource::Replay,
        surface_id: 1,
        time_millis: 1,
        position: [10.0, 20.0],
        surface_size: [100, 100],
        kind: crate::engine::scene::ScenePointerEventKind::Button {
            button: 0x110,
            pressed,
            serial: 1,
        },
    };
    let events = crate::engine::scene::SceneFrameEvents {
        pointer: crate::engine::scene::ScenePointerState {
            sequence: crate::engine::scene::SceneEventSequence(2),
            source: crate::engine::scene::ScenePointerSource::Replay,
            surface_id: 1,
            position: [10.0, 20.0],
            surface_size: [100, 100],
            inside: true,
            ..crate::engine::scene::ScenePointerState::default()
        },
        ordered: vec![
            crate::engine::scene::SceneSequencedEvent {
                sequence: crate::engine::scene::SceneEventSequence(1),
                event: crate::engine::scene::SceneEvent::Pointer(pointer_event(true)),
            },
            crate::engine::scene::SceneSequencedEvent {
                sequence: crate::engine::scene::SceneEventSequence(2),
                event: crate::engine::scene::SceneEvent::Pointer(pointer_event(false)),
            },
        ],
        ..crate::engine::scene::SceneFrameEvents::default()
    };

    let frame = resolver
        .resolve_frame_with_events_at(&world, 1.0, 0.0, &events)
        .expect("clicked frame");
    assert!(!frame.object(SceneObjectHandle(0)).unwrap().resolved_visible);
    assert_eq!(frame.visible_object_count, 0);
}

mod pointer_parallax;
