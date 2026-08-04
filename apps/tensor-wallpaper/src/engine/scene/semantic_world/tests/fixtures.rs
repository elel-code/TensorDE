use super::super::*;
use crate::engine::scene::binary::SceneBinaryDocument;

pub(super) fn semantic_document() -> SceneBinaryDocument {
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

pub(super) fn attachment_document() -> SceneBinaryDocument {
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

pub(super) fn parent_cycle_document() -> SceneBinaryDocument {
    SceneBinaryDocument {
        objects: vec![
            cycle_object(SceneObjectHandle(0), 100, 200),
            cycle_object(SceneObjectHandle(1), 200, 100),
        ],
        ..SceneBinaryDocument::default()
    }
}

pub(super) fn image_object() -> SceneObjectRecord {
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
        camera_zoom: 1.0,
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

pub(super) fn puppet_object() -> SceneObjectRecord {
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
        camera_zoom: 1.0,
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

pub(super) fn parent_puppet_object() -> SceneObjectRecord {
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
        camera_zoom: 1.0,
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

pub(super) fn attached_child_object() -> SceneObjectRecord {
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
        camera_zoom: 1.0,
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

pub(super) fn cycle_object(
    handle: SceneObjectHandle,
    we_id: u32,
    parent_we_id: u32,
) -> SceneObjectRecord {
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
        camera_zoom: 1.0,
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

pub(super) fn image_mesh() -> SceneMeshRecord {
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

pub(super) fn puppet_mesh() -> SceneMeshRecord {
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

pub(super) fn image_mesh_extra() -> SceneMeshRecord {
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

pub(super) fn parent_puppet_mesh() -> SceneMeshRecord {
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

pub(super) fn attached_child_mesh() -> SceneMeshRecord {
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

pub(super) fn effect_pass(pass_index: u32) -> SceneEffectPassRecord {
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

pub(super) fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub(super) fn translated_attachment_matrix(x: f32, y: f32) -> [f32; 16] {
    let mut matrix = identity_matrix();
    matrix[12] = x;
    matrix[13] = y;
    matrix
}

pub(super) fn animation_sample(translation: [f32; 3]) -> ScenePuppetAnimationTransformSampleRecord {
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

pub(super) fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected {actual} to be close to {expected}"
    );
}
