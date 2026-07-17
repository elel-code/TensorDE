use super::*;
use crate::engine::scene::binary::SceneBinaryDocument;

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
fn resolve_frame_multiplies_parent_and_child_visual_state() {
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
            x: 0.1,
            y: 0.3,
            z: 0.8,
        }
    );
    assert!((child.resolved_alpha - 0.2).abs() < f32::EPSILON);
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
        instance_id: 77,
        visible: true,
    });
    document.object_effects.push(SceneObjectEffectRecord {
        object: SceneObjectHandle(1),
        effect: SceneEffectHandle(0),
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
    assert!(frame.object_effects[0].resolved_visible);
    assert_eq!(frame.object_effects[0].pass_start, 0);
    assert_eq!(frame.object_effects[0].pass_count, 2);
    assert_eq!(frame.object_effects[0].fbo_start, 0);
    assert_eq!(frame.object_effects[0].fbo_count, 1);
    assert!(!frame.object_effects[1].resolved_visible);
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
fn retained_audio_binding_applies_uniform_object_scale_before_transform_propagation() {
    let mut object = image_object();
    object.material = SceneMaterialHandle(INVALID_MATERIAL_ID);
    let document = SceneBinaryDocument {
        objects: vec![object],
        audio_band_material_bindings: vec![SceneAudioBandMaterialBindingRecord {
            object: SceneObjectHandle(0),
            target: SceneAudioBandMaterialTarget::ObjectUniformScale,
            spectrum_resolution: 16,
            band_index: 0,
            smoothing: 1.0,
            minimum_multiplier: 1.0,
            maximum_multiplier: 1.5,
            initial_value: 2.0,
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
            spectrum32: [1.0; 32],
            ready: true,
            ..crate::engine::scene::SceneAudioState::default()
        },
        ..crate::engine::scene::SceneFrameEvents::default()
    };
    let frame = resolver
        .resolve_frame_with_events_at(&world, 1.0, &events)
        .expect("audio-scaled frame");
    let object = frame.object(SceneObjectHandle(0)).expect("scaled object");

    assert_close(object.local_matrix[0], 3.0);
    assert_close(object.local_matrix[5], 3.0);
    assert_close(object.local_matrix[10], 3.0);
    assert_close(
        frame
            .audio_material_value(
                SceneObjectHandle(0),
                SceneAudioBandMaterialTarget::ObjectUniformScale,
            )
            .expect("resolved scale"),
        3.0,
    );
}

fn semantic_document() -> SceneBinaryDocument {
    let strings = vec!["root-bone".to_owned(), "weapon".to_owned()];
    SceneBinaryDocument {
        strings,
        objects: vec![image_object(), puppet_object()],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 0,
        }],
        meshes: vec![image_mesh(), puppet_mesh(), image_mesh_extra()],
        mesh_vertices: vec![
            SceneMeshVertexRecord {
                position: SceneVec3::default(),
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            };
            12
        ],
        mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
        puppets: vec![ScenePuppetRecord {
            object: SceneObjectHandle(1),
            resource: SceneResourceId::NONE,
            mesh_start: 1,
            mesh_count: 1,
            bone_start: 0,
            bone_count: 1,
            attachment_start: 0,
            attachment_count: 1,
        }],
        puppet_bones: vec![ScenePuppetBoneRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId(0),
            simulation_type: 0,
            parent_index: -1,
            local_bind_matrix: identity_matrix(),
            simulation_json: SceneStringId::NONE,
        }],
        puppet_attachments: vec![ScenePuppetAttachmentRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId(1),
            local_matrix: identity_matrix(),
        }],
        ..SceneBinaryDocument::default()
    }
}

fn attachment_document() -> SceneBinaryDocument {
    SceneBinaryDocument {
        strings: vec!["root-bone".to_owned(), "eye".to_owned()],
        objects: vec![parent_puppet_object(), attached_child_object()],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 0,
        }],
        meshes: vec![parent_puppet_mesh(), attached_child_mesh()],
        mesh_vertices: vec![
            SceneMeshVertexRecord {
                position: SceneVec3::default(),
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            };
            8
        ],
        mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
        puppets: vec![ScenePuppetRecord {
            object: SceneObjectHandle(0),
            resource: SceneResourceId::NONE,
            mesh_start: 0,
            mesh_count: 1,
            bone_start: 0,
            bone_count: 1,
            attachment_start: 0,
            attachment_count: 1,
        }],
        puppet_bones: vec![ScenePuppetBoneRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId(0),
            simulation_type: 0,
            parent_index: -1,
            local_bind_matrix: identity_matrix(),
            simulation_json: SceneStringId::NONE,
        }],
        puppet_attachments: vec![ScenePuppetAttachmentRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId(1),
            local_matrix: translated_attachment_matrix(2.0, 3.0),
        }],
        ..SceneBinaryDocument::default()
    }
}

fn parent_cycle_document() -> SceneBinaryDocument {
    SceneBinaryDocument {
        objects: vec![
            cycle_object(SceneObjectHandle(0), 100, 200),
            cycle_object(SceneObjectHandle(1), 200, 100),
        ],
        ..SceneBinaryDocument::default()
    }
}

fn image_object() -> SceneObjectRecord {
    SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 100,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(0),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3 {
            x: 10.0,
            y: 20.0,
            z: 0.0,
        },
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 2.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 3,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    }
}

fn puppet_object() -> SceneObjectRecord {
    SceneObjectRecord {
        id: SceneObjectHandle(1),
        we_id: 200,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Puppet,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(0),
        parent_we_id: 100,
        attachment: SceneStringId(1),
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 4,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    }
}

fn parent_puppet_object() -> SceneObjectRecord {
    SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 937,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Puppet,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(0),
        parent_we_id: INVALID_OBJECT_ID,
        attachment: SceneStringId::NONE,
        origin: SceneVec3 {
            x: 10.0,
            y: 20.0,
            z: 0.0,
        },
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 1,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    }
}

fn attached_child_object() -> SceneObjectRecord {
    SceneObjectRecord {
        id: SceneObjectHandle(1),
        we_id: 1336,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(0),
        parent_we_id: 937,
        attachment: SceneStringId(1),
        origin: SceneVec3 {
            x: 5.0,
            y: 7.0,
            z: 0.0,
        },
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 2,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    }
}

fn cycle_object(handle: SceneObjectHandle, we_id: u32, parent_we_id: u32) -> SceneObjectRecord {
    SceneObjectRecord {
        id: handle,
        we_id,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id,
        attachment: SceneStringId::NONE,
        origin: SceneVec3::default(),
        angles: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        color: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        alpha: 1.0,
        visible: true,
        color_blend_mode: 0,
        sort_order: 0,
        effect_start: u32::MAX,
        effect_count: 0,
        render_graph: u32::MAX,
    }
}

fn image_mesh() -> SceneMeshRecord {
    SceneMeshRecord {
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(0),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        width: 64.0,
        height: 32.0,
        bounds_min: SceneVec3 {
            x: -32.0,
            y: -16.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 32.0,
            y: 16.0,
            z: 0.0,
        },
    }
}

fn puppet_mesh() -> SceneMeshRecord {
    SceneMeshRecord {
        object: SceneObjectHandle(1),
        material: SceneMaterialHandle(0),
        vertex_start: 4,
        vertex_count: 4,
        index_start: 6,
        index_count: 6,
        width: 128.0,
        height: 96.0,
        bounds_min: SceneVec3 {
            x: -64.0,
            y: -48.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 64.0,
            y: 48.0,
            z: 0.0,
        },
    }
}

fn image_mesh_extra() -> SceneMeshRecord {
    SceneMeshRecord {
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(0),
        vertex_start: 8,
        vertex_count: 4,
        index_start: 12,
        index_count: 6,
        width: 16.0,
        height: 16.0,
        bounds_min: SceneVec3 {
            x: -8.0,
            y: -8.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 8.0,
            y: 8.0,
            z: 0.0,
        },
    }
}

fn parent_puppet_mesh() -> SceneMeshRecord {
    SceneMeshRecord {
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(0),
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        width: 64.0,
        height: 64.0,
        bounds_min: SceneVec3 {
            x: -32.0,
            y: -32.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 32.0,
            y: 32.0,
            z: 0.0,
        },
    }
}

fn attached_child_mesh() -> SceneMeshRecord {
    SceneMeshRecord {
        object: SceneObjectHandle(1),
        material: SceneMaterialHandle(0),
        vertex_start: 4,
        vertex_count: 4,
        index_start: 6,
        index_count: 6,
        width: 16.0,
        height: 16.0,
        bounds_min: SceneVec3 {
            x: -8.0,
            y: -8.0,
            z: 0.0,
        },
        bounds_max: SceneVec3 {
            x: 8.0,
            y: 8.0,
            z: 0.0,
        },
    }
}

fn effect_pass(pass_index: u32) -> SceneEffectPassRecord {
    SceneEffectPassRecord {
        effect: SceneEffectHandle(0),
        pass_index,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        command: SceneStringId::NONE,
        source: SceneStringId::NONE,
        target: SceneStringId::NONE,
        binding_start: 0,
        binding_count: 0,
        combo_start: 0,
        combo_count: 0,
    }
}

fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn translated_attachment_matrix(x: f32, y: f32) -> [f32; 16] {
    let mut matrix = identity_matrix();
    matrix[12] = x;
    matrix[13] = y;
    matrix
}

fn animation_sample(translation: [f32; 3]) -> ScenePuppetAnimationTransformSampleRecord {
    ScenePuppetAnimationTransformSampleRecord {
        translation: SceneVec3 {
            x: translation[0],
            y: translation[1],
            z: translation[2],
        },
        rotation: SceneVec3::default(),
        scale: SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected {actual} to be close to {expected}"
    );
}
