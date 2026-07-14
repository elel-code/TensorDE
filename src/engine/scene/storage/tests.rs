use super::*;
use crate::engine::scene::binary::{SceneBinaryDocument, write_scene_binary};

#[test]
fn storage_borrows_resource_payload_slices() {
    let mut document = SceneBinaryDocument {
        strings: vec!["scene".to_owned(), "scene.json".to_owned()],
        resource_payload: vec![7, 8, 9],
        ..SceneBinaryDocument::default()
    };
    document.project.wallpaper_type = SceneStringId(0);
    document.project.scene_file = SceneStringId(1);
    document.resources.push(SceneResourceRecord {
        id: SceneResourceId(0),
        kind: SceneResourceKind::SceneJson,
        path: SceneStringId(1),
        source: SceneStringId(1),
        payload_offset: 0,
        payload_len: 3,
    });

    let mut bytes = Vec::new();
    write_scene_binary(&document, &mut bytes).expect("write");
    let mut storage = SceneStorage::from_binary_bytes(&bytes).expect("storage");
    let payload = storage
        .resource_payload(&storage.resources()[0])
        .expect("payload");

    assert_eq!(storage.string(SceneStringId(0)), Some("scene"));
    assert_eq!(payload, &[7, 8, 9]);
    assert_eq!(storage.release_parsed_resource_payload(), 3);
    assert_eq!(storage.resource_payload_bytes(), 0);
    assert_eq!(
        storage.resource_payload(&storage.resources()[0]),
        Some(&[][..])
    );
    validate_document(storage.document()).expect("released storage remains valid");
}

#[test]
fn storage_releases_uploaded_texture_payload_but_keeps_texture_metadata() {
    let resource = SceneResourceId(7);
    let mut document = SceneBinaryDocument {
        texture_payload: vec![1, 2, 3, 4, 5, 6],
        ..SceneBinaryDocument::default()
    };
    document.resources.push(SceneResourceRecord {
        id: resource,
        kind: SceneResourceKind::TextureTex,
        path: SceneStringId::NONE,
        source: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
    });
    document.textures.push(SceneTextureRecord {
        resource,
        format: SceneTextureFormat::R8Unorm,
        source_runtime_format: 9,
        payload_format: 0,
        sampler_flags: 0,
        width: 3,
        height: 2,
        storage_width: 4,
        storage_height: 2,
        mip_start: 0,
        mip_count: 1,
        texv_tag: SceneStringId::NONE,
        texb_tag: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 6,
        alpha_coverage_rows: [u32::MAX;
            crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    });
    document.texture_mips.push(SceneTextureMipRecord {
        width: 3,
        height: 2,
        payload_offset: 0,
        payload_len: 6,
    });
    let mut storage = SceneStorage::from_document(document).expect("storage");

    assert_eq!(storage.release_uploaded_texture_payload(), 6);
    assert_eq!(storage.texture_payload_bytes(), 0);
    assert_eq!(storage.textures()[0].width, 3);
    assert_eq!(storage.textures()[0].storage_width, 4);
    assert!(storage.texture_payload(&storage.textures()[0]).is_empty());
    assert!(
        storage
            .texture_mip_payload(&storage.document.texture_mips[0])
            .is_empty()
    );
    validate_document(storage.document()).expect("released storage remains valid");
}

#[test]
fn storage_rejects_invalid_material_handles() {
    let mut document = SceneBinaryDocument {
        strings: vec!["scene".to_owned(), "scene.json".to_owned()],
        ..SceneBinaryDocument::default()
    };
    document.project.wallpaper_type = SceneStringId(0);
    document.project.scene_file = SceneStringId(1);
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(42),
        parent_we_id: INVALID_OBJECT_ID,
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
    });

    let err = SceneStorage::from_document(document).expect_err("invalid material");

    assert!(matches!(
        err,
        SceneStorageError::InvalidMaterialHandle {
            field: "object.material",
            handle: SceneMaterialHandle(42)
        }
    ));
}

#[test]
fn storage_rejects_mesh_indices_outside_local_vertex_range() {
    let mut document = SceneBinaryDocument::default();
    document.objects.push(SceneObjectRecord {
        id: SceneObjectHandle(0),
        we_id: 1,
        name: SceneStringId::NONE,
        kind: SceneObjectKind::Image,
        resource: SceneResourceId::NONE,
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        parent_we_id: INVALID_OBJECT_ID,
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
    });
    document.meshes.push(SceneMeshRecord {
        object: SceneObjectHandle(0),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
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
    });
    document.mesh_vertices.resize(
        4,
        SceneMeshVertexRecord {
            position: SceneVec3::default(),
            uv: [0.0, 0.0],
            blend_indices: [0; 4],
            blend_weights: [0.0; 4],
        },
    );
    document.mesh_indices = vec![0, 1, 2, 0, 2, 4];

    let err = SceneStorage::from_document(document).expect_err("invalid mesh index");

    assert!(matches!(
        err,
        SceneStorageError::InvalidMeshIndex {
            mesh: 0,
            index: 4,
            vertex_count: 4
        }
    ));
}
