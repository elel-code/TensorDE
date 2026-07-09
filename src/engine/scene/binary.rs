//! New Gilder scene engine binary reader/writer.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/project-format.md`
//! - `reverse-engineered/docs/scene-pkg-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/servers/rendering/storage/*`

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};

use super::abi::*;

const HEADER_LEN: usize = 36;
const CHUNK_ENTRY_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct SceneBinaryDocument {
    pub feature_flags: u64,
    pub strings: Vec<String>,
    pub project: SceneProjectRecord,
    pub resources: Vec<SceneResourceRecord>,
    pub resource_payload: Vec<u8>,
    pub textures: Vec<SceneTextureRecord>,
    pub objects: Vec<SceneObjectRecord>,
    pub object_effects: Vec<SceneObjectEffectRecord>,
    pub materials: Vec<SceneMaterialRecord>,
    pub material_passes: Vec<SceneMaterialPassRecord>,
    pub material_textures: Vec<SceneMaterialTextureRecord>,
    pub material_constants: Vec<SceneMaterialConstantRecord>,
    pub meshes: Vec<SceneMeshRecord>,
    pub mesh_vertices: Vec<SceneMeshVertexRecord>,
    pub mesh_indices: Vec<u32>,
    pub effects: Vec<SceneEffectRecord>,
    pub effect_passes: Vec<SceneEffectPassRecord>,
    pub effect_bindings: Vec<SceneEffectBindingRecord>,
    pub effect_combos: Vec<SceneEffectComboRecord>,
    pub effect_fbos: Vec<SceneEffectFboRecord>,
    pub render_graphs: Vec<SceneRenderGraphRecord>,
    pub render_passes: Vec<SceneRenderPassRecord>,
    pub render_bindings: Vec<SceneRenderBindingRecord>,
    pub unsupported: Vec<SceneUnsupportedRecord>,
    pub image_targets: Vec<SceneImageTargetRecord>,
    pub shader_contracts: Vec<SceneShaderContractRecord>,
    pub shader_constant_names: Vec<SceneStringId>,
}

impl Default for SceneBinaryDocument {
    fn default() -> Self {
        Self {
            feature_flags: SCENE_DEFAULT_FEATURE_FLAGS,
            strings: Vec::new(),
            project: empty_project_record(),
            resources: Vec::new(),
            resource_payload: Vec::new(),
            textures: Vec::new(),
            objects: Vec::new(),
            object_effects: Vec::new(),
            materials: Vec::new(),
            material_passes: Vec::new(),
            material_textures: Vec::new(),
            material_constants: Vec::new(),
            meshes: Vec::new(),
            mesh_vertices: Vec::new(),
            mesh_indices: Vec::new(),
            effects: Vec::new(),
            effect_passes: Vec::new(),
            effect_bindings: Vec::new(),
            effect_combos: Vec::new(),
            effect_fbos: Vec::new(),
            render_graphs: Vec::new(),
            render_passes: Vec::new(),
            render_bindings: Vec::new(),
            unsupported: Vec::new(),
            image_targets: Vec::new(),
            shader_contracts: Vec::new(),
            shader_constant_names: Vec::new(),
        }
    }
}

fn empty_project_record() -> SceneProjectRecord {
    SceneProjectRecord {
        title: SceneStringId::NONE,
        wallpaper_type: SceneStringId::NONE,
        scene_file: SceneStringId::NONE,
        preview: SceneStringId::NONE,
        properties_json: SceneStringId::NONE,
        logical_width: 0,
        logical_height: 0,
        clear_color: [0.0, 0.0, 0.0, 1.0],
        ambient_color: [0.0, 0.0, 0.0, 1.0],
        skylight_color: [0.0, 0.0, 0.0, 1.0],
        camera_eye: SceneVec3::default(),
        camera_center: SceneVec3 {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        },
        camera_up: SceneVec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    }
}

pub fn write_scene_binary(
    document: &SceneBinaryDocument,
    mut writer: impl Write,
) -> Result<(), SceneBinaryError> {
    let chunks = encode_chunks(document)?;
    let table_len = chunks
        .len()
        .checked_mul(CHUNK_ENTRY_LEN)
        .ok_or(SceneBinaryError::SizeOverflow("chunk table"))?;
    let payload_start = HEADER_LEN
        .checked_add(table_len)
        .ok_or(SceneBinaryError::SizeOverflow("payload start"))?;
    let mut payload_offset = payload_start as u64;

    let mut output = Vec::with_capacity(payload_start);
    output.extend_from_slice(&SCENE_BINARY_MAGIC);
    put_u32(&mut output, SCENE_BINARY_VERSION);
    output.push(SCENE_BINARY_ENDIANNESS_LITTLE);
    output.extend_from_slice(&[0, 0, 0]);
    put_u64(&mut output, document.feature_flags);
    put_u32(&mut output, checked_u32(chunks.len(), "chunk count")?);
    put_u64(&mut output, HEADER_LEN as u64);

    for chunk in &chunks {
        put_u32(&mut output, chunk.kind);
        put_u32(&mut output, 0);
        put_u64(&mut output, payload_offset);
        put_u64(&mut output, checked_u64(chunk.data.len(), "chunk length")?);
        put_u32(&mut output, chunk.item_count);
        put_u32(&mut output, 0);
        payload_offset = payload_offset
            .checked_add(checked_u64(chunk.data.len(), "chunk length")?)
            .ok_or(SceneBinaryError::SizeOverflow("chunk payload offset"))?;
    }

    for chunk in chunks {
        output.extend_from_slice(&chunk.data);
    }

    writer.write_all(&output).map_err(SceneBinaryError::Write)
}

pub fn read_scene_binary(mut reader: impl Read) -> Result<SceneBinaryDocument, SceneBinaryError> {
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(SceneBinaryError::Read)?;
    read_scene_binary_bytes(&data)
}

pub fn read_scene_binary_bytes(data: &[u8]) -> Result<SceneBinaryDocument, SceneBinaryError> {
    if data.len() < HEADER_LEN {
        return Err(SceneBinaryError::Truncated("header"));
    }
    if data[0..8] != SCENE_BINARY_MAGIC {
        return Err(SceneBinaryError::InvalidMagic);
    }
    let version = read_u32_at(data, 8, "version")?;
    if version != SCENE_BINARY_VERSION {
        return Err(SceneBinaryError::UnsupportedVersion(version));
    }
    if data[12] != SCENE_BINARY_ENDIANNESS_LITTLE {
        return Err(SceneBinaryError::UnsupportedEndianness(data[12]));
    }
    let feature_flags = read_u64_at(data, 16, "feature flags")?;
    let chunk_count = read_u32_at(data, 24, "chunk count")? as usize;
    let table_offset = read_u64_at(data, 28, "chunk table offset")? as usize;
    let table_len = chunk_count
        .checked_mul(CHUNK_ENTRY_LEN)
        .ok_or(SceneBinaryError::SizeOverflow("chunk table"))?;
    let table_end = table_offset
        .checked_add(table_len)
        .ok_or(SceneBinaryError::SizeOverflow("chunk table end"))?;
    if table_end > data.len() {
        return Err(SceneBinaryError::Truncated("chunk table"));
    }

    let mut chunks = BTreeMap::new();
    for index in 0..chunk_count {
        let base = table_offset + index * CHUNK_ENTRY_LEN;
        let kind = read_u32_at(data, base, "chunk kind")?;
        let offset = read_u64_at(data, base + 8, "chunk offset")? as usize;
        let len = read_u64_at(data, base + 16, "chunk length")? as usize;
        let item_count = read_u32_at(data, base + 24, "chunk item count")?;
        let end = offset
            .checked_add(len)
            .ok_or(SceneBinaryError::SizeOverflow("chunk end"))?;
        if end > data.len() {
            return Err(SceneBinaryError::Truncated("chunk payload"));
        }
        if chunks
            .insert(
                kind,
                SceneDecodedChunk {
                    payload: &data[offset..end],
                    item_count,
                },
            )
            .is_some()
        {
            return Err(SceneBinaryError::DuplicateChunk(kind));
        }
    }
    validate_required_chunks(&chunks)?;

    let strings = decode_string_table(chunk_payload(&chunks, CHUNK_STRING_TABLE)?)?;
    ensure_chunk_count(&chunks, CHUNK_STRING_TABLE, "string table", strings.len())?;
    let project = decode_project(chunk_payload(&chunks, CHUNK_PROJECT)?)?;
    ensure_chunk_count(&chunks, CHUNK_PROJECT, "project", 1)?;
    let resources = decode_resources(chunk_payload(&chunks, CHUNK_RESOURCE)?)?;
    ensure_chunk_count(&chunks, CHUNK_RESOURCE, "resources", resources.len())?;
    let resource_payload = chunk_payload(&chunks, CHUNK_RESOURCE_PAYLOAD)?.to_vec();
    ensure_chunk_count(
        &chunks,
        CHUNK_RESOURCE_PAYLOAD,
        "resource payload owners",
        resources.len(),
    )?;
    let textures = decode_textures(chunk_payload(&chunks, CHUNK_TEXTURE)?)?;
    ensure_chunk_count(&chunks, CHUNK_TEXTURE, "textures", textures.len())?;
    let (objects, object_effects) =
        decode_scene_objects(chunk_payload(&chunks, CHUNK_SCENE_OBJECT)?)?;
    ensure_chunk_count(&chunks, CHUNK_SCENE_OBJECT, "scene objects", objects.len())?;
    let (materials, material_passes, material_textures, material_constants) =
        decode_materials(chunk_payload(&chunks, CHUNK_MATERIAL)?)?;
    ensure_chunk_count(&chunks, CHUNK_MATERIAL, "materials", materials.len())?;
    let (meshes, mesh_vertices, mesh_indices) = decode_meshes(chunk_payload(&chunks, CHUNK_MESH)?)?;
    ensure_chunk_count(&chunks, CHUNK_MESH, "mesh", meshes.len())?;
    let (effects, effect_passes, effect_bindings, effect_combos, effect_fbos) =
        decode_effects(chunk_payload(&chunks, CHUNK_EFFECT)?)?;
    ensure_chunk_count(&chunks, CHUNK_EFFECT, "effects", effects.len())?;
    let (render_graphs, render_passes, render_bindings, unsupported) =
        decode_render_graphs(chunk_payload(&chunks, CHUNK_RENDER_GRAPH)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_RENDER_GRAPH,
        "render graphs",
        render_graphs.len(),
    )?;
    let image_targets = decode_image_targets(chunk_payload(&chunks, CHUNK_IMAGE_TARGET)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_IMAGE_TARGET,
        "image targets",
        image_targets.len(),
    )?;
    let (shader_contracts, shader_constant_names) =
        decode_shader_contracts(chunk_payload(&chunks, CHUNK_SHADER_CONTRACT)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_SHADER_CONTRACT,
        "shader contracts",
        shader_contracts.len(),
    )?;
    for &(kind, name) in &[
        (CHUNK_TIMELINE, "timeline"),
        (CHUNK_PUPPET, "puppet"),
        (CHUNK_PARTICLE, "particle"),
        (CHUNK_AUDIO, "audio"),
        (CHUNK_SCRIPT_BINDING, "script binding"),
    ] {
        ensure_chunk_count(&chunks, kind, name, 0)?;
    }

    Ok(SceneBinaryDocument {
        feature_flags,
        strings,
        project,
        resources,
        resource_payload,
        textures,
        objects,
        object_effects,
        materials,
        material_passes,
        material_textures,
        material_constants,
        meshes,
        mesh_vertices,
        mesh_indices,
        effects,
        effect_passes,
        effect_bindings,
        effect_combos,
        effect_fbos,
        render_graphs,
        render_passes,
        render_bindings,
        unsupported,
        image_targets,
        shader_contracts,
        shader_constant_names,
    })
}

#[derive(Debug)]
pub enum SceneBinaryError {
    Read(io::Error),
    Write(io::Error),
    InvalidMagic,
    UnsupportedVersion(u32),
    UnsupportedEndianness(u8),
    MissingChunk(u32),
    DuplicateChunk(u32),
    InvalidChunkValue(&'static str, u32),
    InvalidUtf8(std::string::FromUtf8Error),
    Truncated(&'static str),
    SizeOverflow(&'static str),
    CountMismatch {
        chunk: &'static str,
        expected: u32,
        actual: usize,
    },
}

impl fmt::Display for SceneBinaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read scene binary: {err}"),
            Self::Write(err) => write!(f, "failed to write scene binary: {err}"),
            Self::InvalidMagic => f.write_str("invalid Gilder scene binary magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported Gilder scene binary version {version}")
            }
            Self::UnsupportedEndianness(value) => {
                write!(f, "unsupported Gilder scene binary endianness tag {value}")
            }
            Self::MissingChunk(kind) => write!(f, "scene binary missing required chunk {kind:#x}"),
            Self::DuplicateChunk(kind) => write!(f, "scene binary has duplicate chunk {kind:#x}"),
            Self::InvalidChunkValue(name, value) => {
                write!(f, "invalid {name} value in scene binary: {value}")
            }
            Self::InvalidUtf8(err) => write!(f, "invalid UTF-8 in scene binary: {err}"),
            Self::Truncated(name) => write!(f, "truncated scene binary {name}"),
            Self::SizeOverflow(name) => write!(f, "scene binary size overflow in {name}"),
            Self::CountMismatch {
                chunk,
                expected,
                actual,
            } => write!(
                f,
                "scene binary {chunk} count mismatch: table says {expected}, payload has {actual}"
            ),
        }
    }
}

impl std::error::Error for SceneBinaryError {}

struct SceneEncodedChunk {
    kind: u32,
    item_count: u32,
    data: Vec<u8>,
}

struct SceneDecodedChunk<'a> {
    payload: &'a [u8],
    item_count: u32,
}

fn encode_chunks(
    document: &SceneBinaryDocument,
) -> Result<Vec<SceneEncodedChunk>, SceneBinaryError> {
    let chunks = vec![
        SceneEncodedChunk {
            kind: CHUNK_STRING_TABLE,
            item_count: checked_u32(document.strings.len(), "string count")?,
            data: encode_string_table(&document.strings)?,
        },
        SceneEncodedChunk {
            kind: CHUNK_PROJECT,
            item_count: 1,
            data: encode_project(&document.project),
        },
        SceneEncodedChunk {
            kind: CHUNK_SCENE_OBJECT,
            item_count: checked_u32(document.objects.len(), "object count")?,
            data: encode_scene_objects(&document.objects, &document.object_effects)?,
        },
        SceneEncodedChunk {
            kind: CHUNK_RESOURCE,
            item_count: checked_u32(document.resources.len(), "resource count")?,
            data: encode_resources(&document.resources),
        },
        SceneEncodedChunk {
            kind: CHUNK_RESOURCE_PAYLOAD,
            item_count: checked_u32(document.resources.len(), "resource payload owner count")?,
            data: document.resource_payload.clone(),
        },
        SceneEncodedChunk {
            kind: CHUNK_TEXTURE,
            item_count: checked_u32(document.textures.len(), "texture count")?,
            data: encode_textures(&document.textures),
        },
        SceneEncodedChunk {
            kind: CHUNK_MATERIAL,
            item_count: checked_u32(document.materials.len(), "material count")?,
            data: encode_materials(
                &document.materials,
                &document.material_passes,
                &document.material_textures,
                &document.material_constants,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_MESH,
            item_count: checked_u32(document.meshes.len(), "mesh count")?,
            data: encode_meshes(
                &document.meshes,
                &document.mesh_vertices,
                &document.mesh_indices,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_EFFECT,
            item_count: checked_u32(document.effects.len(), "effect count")?,
            data: encode_effects(
                &document.effects,
                &document.effect_passes,
                &document.effect_bindings,
                &document.effect_combos,
                &document.effect_fbos,
            )?,
        },
        empty_count_chunk(CHUNK_TIMELINE),
        empty_count_chunk(CHUNK_PUPPET),
        empty_count_chunk(CHUNK_PARTICLE),
        empty_count_chunk(CHUNK_AUDIO),
        empty_count_chunk(CHUNK_SCRIPT_BINDING),
        SceneEncodedChunk {
            kind: CHUNK_RENDER_GRAPH,
            item_count: checked_u32(document.render_graphs.len(), "render graph count")?,
            data: encode_render_graphs(
                &document.render_graphs,
                &document.render_passes,
                &document.render_bindings,
                &document.unsupported,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_IMAGE_TARGET,
            item_count: checked_u32(document.image_targets.len(), "image target count")?,
            data: encode_image_targets(&document.image_targets),
        },
        SceneEncodedChunk {
            kind: CHUNK_SHADER_CONTRACT,
            item_count: checked_u32(document.shader_contracts.len(), "shader contract count")?,
            data: encode_shader_contracts(
                &document.shader_contracts,
                &document.shader_constant_names,
            )?,
        },
    ];
    Ok(chunks)
}

fn empty_count_chunk(kind: u32) -> SceneEncodedChunk {
    let mut data = Vec::new();
    put_u32(&mut data, 0);
    SceneEncodedChunk {
        kind,
        item_count: 0,
        data,
    }
}

fn validate_required_chunks(
    chunks: &BTreeMap<u32, SceneDecodedChunk<'_>>,
) -> Result<(), SceneBinaryError> {
    let present = chunks.keys().copied().collect::<BTreeSet<_>>();
    for &kind in REQUIRED_SCENE_CHUNKS {
        if !present.contains(&kind) {
            return Err(SceneBinaryError::MissingChunk(kind));
        }
    }
    Ok(())
}

fn chunk_payload<'a>(
    chunks: &'a BTreeMap<u32, SceneDecodedChunk<'a>>,
    kind: u32,
) -> Result<&'a [u8], SceneBinaryError> {
    chunks
        .get(&kind)
        .map(|chunk| {
            let _ = chunk.item_count;
            chunk.payload
        })
        .ok_or(SceneBinaryError::MissingChunk(kind))
}

fn ensure_chunk_count(
    chunks: &BTreeMap<u32, SceneDecodedChunk<'_>>,
    kind: u32,
    chunk: &'static str,
    actual: usize,
) -> Result<(), SceneBinaryError> {
    let expected = chunks
        .get(&kind)
        .ok_or(SceneBinaryError::MissingChunk(kind))?
        .item_count;
    if expected as usize == actual {
        Ok(())
    } else {
        Err(SceneBinaryError::CountMismatch {
            chunk,
            expected,
            actual,
        })
    }
}

fn encode_string_table(strings: &[String]) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(strings.len(), "string count")?);
    for value in strings {
        let bytes = value.as_bytes();
        put_u32(&mut out, checked_u32(bytes.len(), "string length")?);
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

fn decode_string_table(data: &[u8]) -> Result<Vec<String>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut strings = Vec::with_capacity(count);
    for _ in 0..count {
        let len = decoder.u32()? as usize;
        strings.push(
            String::from_utf8(decoder.bytes(len)?.to_vec())
                .map_err(SceneBinaryError::InvalidUtf8)?,
        );
    }
    Ok(strings)
}

fn encode_project(project: &SceneProjectRecord) -> Vec<u8> {
    let mut out = Vec::new();
    put_string_id(&mut out, project.title);
    put_string_id(&mut out, project.wallpaper_type);
    put_string_id(&mut out, project.scene_file);
    put_string_id(&mut out, project.preview);
    put_string_id(&mut out, project.properties_json);
    put_u32(&mut out, project.logical_width);
    put_u32(&mut out, project.logical_height);
    put_f32_array(&mut out, &project.clear_color);
    put_f32_array(&mut out, &project.ambient_color);
    put_f32_array(&mut out, &project.skylight_color);
    put_vec3(&mut out, project.camera_eye);
    put_vec3(&mut out, project.camera_center);
    put_vec3(&mut out, project.camera_up);
    out
}

fn decode_project(data: &[u8]) -> Result<SceneProjectRecord, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    Ok(SceneProjectRecord {
        title: decoder.string_id()?,
        wallpaper_type: decoder.string_id()?,
        scene_file: decoder.string_id()?,
        preview: decoder.string_id()?,
        properties_json: decoder.string_id()?,
        logical_width: decoder.u32()?,
        logical_height: decoder.u32()?,
        clear_color: decoder.f32_array4()?,
        ambient_color: decoder.f32_array4()?,
        skylight_color: decoder.f32_array4()?,
        camera_eye: decoder.vec3()?,
        camera_center: decoder.vec3()?,
        camera_up: decoder.vec3()?,
    })
}

fn encode_resources(resources: &[SceneResourceRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, resources.len() as u32);
    for record in resources {
        put_resource_id(&mut out, record.id);
        put_u32(&mut out, record.kind.to_u32());
        put_string_id(&mut out, record.path);
        put_string_id(&mut out, record.source);
        put_u64(&mut out, record.payload_offset);
        put_u64(&mut out, record.payload_len);
    }
    out
}

fn decode_resources(data: &[u8]) -> Result<Vec<SceneResourceRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let id = decoder.resource_id()?;
        let kind_raw = decoder.u32()?;
        let kind = SceneResourceKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("resource kind", kind_raw),
        )?;
        records.push(SceneResourceRecord {
            id,
            kind,
            path: decoder.string_id()?,
            source: decoder.string_id()?,
            payload_offset: decoder.u64()?,
            payload_len: decoder.u64()?,
        });
    }
    Ok(records)
}

fn encode_textures(textures: &[SceneTextureRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, textures.len() as u32);
    for record in textures {
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.format);
        put_u32(&mut out, record.width);
        put_u32(&mut out, record.height);
        put_u32(&mut out, record.storage_width);
        put_u32(&mut out, record.storage_height);
        put_u32(&mut out, record.mip_count);
        put_string_id(&mut out, record.texv_tag);
        put_string_id(&mut out, record.texb_tag);
        put_u64(&mut out, record.payload_offset);
        put_u64(&mut out, record.payload_len);
    }
    out
}

fn decode_textures(data: &[u8]) -> Result<Vec<SceneTextureRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        records.push(SceneTextureRecord {
            resource: decoder.resource_id()?,
            format: decoder.u32()?,
            width: decoder.u32()?,
            height: decoder.u32()?,
            storage_width: decoder.u32()?,
            storage_height: decoder.u32()?,
            mip_count: decoder.u32()?,
            texv_tag: decoder.string_id()?,
            texb_tag: decoder.string_id()?,
            payload_offset: decoder.u64()?,
            payload_len: decoder.u64()?,
        });
    }
    Ok(records)
}

fn encode_scene_objects(
    objects: &[SceneObjectRecord],
    object_effects: &[SceneObjectEffectRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(objects.len(), "object count")?);
    for record in objects {
        put_u32(&mut out, record.id.0);
        put_u32(&mut out, record.we_id);
        put_string_id(&mut out, record.name);
        put_u32(&mut out, record.kind.to_u32());
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.parent_we_id);
        put_string_id(&mut out, record.attachment);
        put_vec3(&mut out, record.origin);
        put_vec3(&mut out, record.angles);
        put_vec3(&mut out, record.scale);
        put_bool(&mut out, record.visible);
        put_i32(&mut out, record.color_blend_mode);
        put_i32(&mut out, record.sort_order);
        put_u32(&mut out, record.effect_start);
        put_u32(&mut out, record.effect_count);
        put_u32(&mut out, record.render_graph);
    }
    put_u32(
        &mut out,
        checked_u32(object_effects.len(), "object effect count")?,
    );
    for record in object_effects {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.effect.0);
        put_u32(&mut out, record.instance_id);
        put_bool(&mut out, record.visible);
    }
    Ok(out)
}

fn decode_scene_objects(
    data: &[u8],
) -> Result<(Vec<SceneObjectRecord>, Vec<SceneObjectEffectRecord>), SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let object_count = decoder.u32()? as usize;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        let id = SceneObjectHandle(decoder.u32()?);
        let we_id = decoder.u32()?;
        let name = decoder.string_id()?;
        let kind_raw = decoder.u32()?;
        let kind = SceneObjectKind::from_u32(kind_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("object kind", kind_raw))?;
        objects.push(SceneObjectRecord {
            id,
            we_id,
            name,
            kind,
            resource: decoder.resource_id()?,
            material: SceneMaterialHandle(decoder.u32()?),
            parent_we_id: decoder.u32()?,
            attachment: decoder.string_id()?,
            origin: decoder.vec3()?,
            angles: decoder.vec3()?,
            scale: decoder.vec3()?,
            visible: decoder.bool()?,
            color_blend_mode: decoder.i32()?,
            sort_order: decoder.i32()?,
            effect_start: decoder.u32()?,
            effect_count: decoder.u32()?,
            render_graph: decoder.u32()?,
        });
    }
    let effect_count = decoder.u32()? as usize;
    let mut object_effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        object_effects.push(SceneObjectEffectRecord {
            object: SceneObjectHandle(decoder.u32()?),
            effect: SceneEffectHandle(decoder.u32()?),
            instance_id: decoder.u32()?,
            visible: decoder.bool()?,
        });
    }
    Ok((objects, object_effects))
}

fn encode_materials(
    materials: &[SceneMaterialRecord],
    passes: &[SceneMaterialPassRecord],
    textures: &[SceneMaterialTextureRecord],
    constants: &[SceneMaterialConstantRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(materials.len(), "material count")?);
    for record in materials {
        put_u32(&mut out, record.id.0);
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "material pass count")?);
    for record in passes {
        put_u32(&mut out, record.material.0);
        put_string_id(&mut out, record.shader_key);
        put_string_id(&mut out, record.target);
        put_u32(&mut out, record.texture_start);
        put_u32(&mut out, record.texture_count);
        put_u32(&mut out, record.constant_start);
        put_u32(&mut out, record.constant_count);
        put_u32(&mut out, record.pipeline_blend.to_u32());
        put_u32(&mut out, record.depth_test.to_u32());
        put_bool(&mut out, record.depth_write);
        put_u32(&mut out, record.cull_mode.to_u32());
        put_string_id(&mut out, record.alpha_writing);
        put_bool(&mut out, record.clear_target);
    }
    put_u32(
        &mut out,
        checked_u32(textures.len(), "material texture count")?,
    );
    for record in textures {
        put_u32(&mut out, record.slot);
        put_resource_id(&mut out, record.resource);
        put_string_id(&mut out, record.path);
    }
    put_u32(
        &mut out,
        checked_u32(constants.len(), "material constant count")?,
    );
    for record in constants {
        put_string_id(&mut out, record.name);
        put_string_id(&mut out, record.value_json);
    }
    Ok(out)
}

type MaterialDecode = (
    Vec<SceneMaterialRecord>,
    Vec<SceneMaterialPassRecord>,
    Vec<SceneMaterialTextureRecord>,
    Vec<SceneMaterialConstantRecord>,
);

fn decode_materials(data: &[u8]) -> Result<MaterialDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let material_count = decoder.u32()? as usize;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        materials.push(SceneMaterialRecord {
            id: SceneMaterialHandle(decoder.u32()?),
            resource: decoder.resource_id()?,
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        let material = SceneMaterialHandle(decoder.u32()?);
        let shader_key = decoder.string_id()?;
        let target = decoder.string_id()?;
        let texture_start = decoder.u32()?;
        let texture_count = decoder.u32()?;
        let constant_start = decoder.u32()?;
        let constant_count = decoder.u32()?;
        let blend_raw = decoder.u32()?;
        let pipeline_blend = ScenePipelineBlend::from_u32(blend_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("pipeline blend", blend_raw),
        )?;
        let depth_raw = decoder.u32()?;
        let depth_test = SceneDepthTest::from_u32(depth_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("depth test", depth_raw))?;
        let depth_write = decoder.bool()?;
        let cull_raw = decoder.u32()?;
        let cull_mode = SceneCullMode::from_u32(cull_raw)
            .ok_or(SceneBinaryError::InvalidChunkValue("cull mode", cull_raw))?;
        passes.push(SceneMaterialPassRecord {
            material,
            shader_key,
            target,
            texture_start,
            texture_count,
            constant_start,
            constant_count,
            pipeline_blend,
            depth_test,
            depth_write,
            cull_mode,
            alpha_writing: decoder.string_id()?,
            clear_target: decoder.bool()?,
        });
    }
    let texture_count = decoder.u32()? as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        textures.push(SceneMaterialTextureRecord {
            slot: decoder.u32()?,
            resource: decoder.resource_id()?,
            path: decoder.string_id()?,
        });
    }
    let constant_count = decoder.u32()? as usize;
    let mut constants = Vec::with_capacity(constant_count);
    for _ in 0..constant_count {
        constants.push(SceneMaterialConstantRecord {
            name: decoder.string_id()?,
            value_json: decoder.string_id()?,
        });
    }
    Ok((materials, passes, textures, constants))
}

fn encode_meshes(
    meshes: &[SceneMeshRecord],
    vertices: &[SceneMeshVertexRecord],
    indices: &[u32],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(meshes.len(), "mesh count")?);
    for record in meshes {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.vertex_start);
        put_u32(&mut out, record.vertex_count);
        put_u32(&mut out, record.index_start);
        put_u32(&mut out, record.index_count);
        put_f32(&mut out, record.width);
        put_f32(&mut out, record.height);
        put_vec3(&mut out, record.bounds_min);
        put_vec3(&mut out, record.bounds_max);
    }
    put_u32(&mut out, checked_u32(vertices.len(), "mesh vertex count")?);
    for vertex in vertices {
        put_vec3(&mut out, vertex.position);
        put_f32(&mut out, vertex.uv[0]);
        put_f32(&mut out, vertex.uv[1]);
    }
    put_u32(&mut out, checked_u32(indices.len(), "mesh index count")?);
    for index in indices {
        put_u32(&mut out, *index);
    }
    Ok(out)
}

fn decode_meshes(
    data: &[u8],
) -> Result<(Vec<SceneMeshRecord>, Vec<SceneMeshVertexRecord>, Vec<u32>), SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let mesh_count = decoder.u32()? as usize;
    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        meshes.push(SceneMeshRecord {
            object: SceneObjectHandle(decoder.u32()?),
            material: SceneMaterialHandle(decoder.u32()?),
            vertex_start: decoder.u32()?,
            vertex_count: decoder.u32()?,
            index_start: decoder.u32()?,
            index_count: decoder.u32()?,
            width: decoder.f32()?,
            height: decoder.f32()?,
            bounds_min: decoder.vec3()?,
            bounds_max: decoder.vec3()?,
        });
    }
    let vertex_count = decoder.u32()? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(SceneMeshVertexRecord {
            position: decoder.vec3()?,
            uv: [decoder.f32()?, decoder.f32()?],
        });
    }
    let index_count = decoder.u32()? as usize;
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(decoder.u32()?);
    }
    Ok((meshes, vertices, indices))
}

fn encode_effects(
    effects: &[SceneEffectRecord],
    passes: &[SceneEffectPassRecord],
    bindings: &[SceneEffectBindingRecord],
    combos: &[SceneEffectComboRecord],
    fbos: &[SceneEffectFboRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(effects.len(), "effect count")?);
    for record in effects {
        put_u32(&mut out, record.id.0);
        put_resource_id(&mut out, record.resource);
        put_string_id(&mut out, record.replacement_key);
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
        put_u32(&mut out, record.fbo_start);
        put_u32(&mut out, record.fbo_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "effect pass count")?);
    for record in passes {
        put_u32(&mut out, record.effect.0);
        put_u32(&mut out, record.pass_index);
        put_u32(&mut out, record.material.0);
        put_string_id(&mut out, record.command);
        put_string_id(&mut out, record.source);
        put_string_id(&mut out, record.target);
        put_u32(&mut out, record.binding_start);
        put_u32(&mut out, record.binding_count);
        put_u32(&mut out, record.combo_start);
        put_u32(&mut out, record.combo_count);
    }
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "effect binding count")?,
    );
    for record in bindings {
        put_u32(&mut out, record.slot);
        put_string_id(&mut out, record.target);
    }
    put_u32(&mut out, checked_u32(combos.len(), "effect combo count")?);
    for record in combos {
        put_string_id(&mut out, record.name);
        put_i64(&mut out, record.value);
    }
    put_u32(&mut out, checked_u32(fbos.len(), "effect fbo count")?);
    for record in fbos {
        put_string_id(&mut out, record.name);
        put_string_id(&mut out, record.format);
        put_f32(&mut out, record.scale);
    }
    Ok(out)
}

type EffectDecode = (
    Vec<SceneEffectRecord>,
    Vec<SceneEffectPassRecord>,
    Vec<SceneEffectBindingRecord>,
    Vec<SceneEffectComboRecord>,
    Vec<SceneEffectFboRecord>,
);

fn decode_effects(data: &[u8]) -> Result<EffectDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let effect_count = decoder.u32()? as usize;
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(SceneEffectRecord {
            id: SceneEffectHandle(decoder.u32()?),
            resource: decoder.resource_id()?,
            replacement_key: decoder.string_id()?,
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
            fbo_start: decoder.u32()?,
            fbo_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        passes.push(SceneEffectPassRecord {
            effect: SceneEffectHandle(decoder.u32()?),
            pass_index: decoder.u32()?,
            material: SceneMaterialHandle(decoder.u32()?),
            command: decoder.string_id()?,
            source: decoder.string_id()?,
            target: decoder.string_id()?,
            binding_start: decoder.u32()?,
            binding_count: decoder.u32()?,
            combo_start: decoder.u32()?,
            combo_count: decoder.u32()?,
        });
    }
    let binding_count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(binding_count);
    for _ in 0..binding_count {
        bindings.push(SceneEffectBindingRecord {
            slot: decoder.u32()?,
            target: decoder.string_id()?,
        });
    }
    let combo_count = decoder.u32()? as usize;
    let mut combos = Vec::with_capacity(combo_count);
    for _ in 0..combo_count {
        combos.push(SceneEffectComboRecord {
            name: decoder.string_id()?,
            value: decoder.i64()?,
        });
    }
    let fbo_count = decoder.u32()? as usize;
    let mut fbos = Vec::with_capacity(fbo_count);
    for _ in 0..fbo_count {
        fbos.push(SceneEffectFboRecord {
            name: decoder.string_id()?,
            format: decoder.string_id()?,
            scale: decoder.f32()?,
        });
    }
    Ok((effects, passes, bindings, combos, fbos))
}

fn encode_render_graphs(
    graphs: &[SceneRenderGraphRecord],
    passes: &[SceneRenderPassRecord],
    bindings: &[SceneRenderBindingRecord],
    unsupported: &[SceneUnsupportedRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(graphs.len(), "render graph count")?);
    for record in graphs {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.pass_start);
        put_u32(&mut out, record.pass_count);
        put_u32(&mut out, record.unsupported_start);
        put_u32(&mut out, record.unsupported_count);
    }
    put_u32(&mut out, checked_u32(passes.len(), "render pass count")?);
    for record in passes {
        put_u32(&mut out, record.id);
        put_u32(&mut out, record.role.to_u32());
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.pass_index);
        put_string_id(&mut out, record.shader_key);
        put_u32(&mut out, record.target.to_u32());
        put_string_id(&mut out, record.target_name);
        put_u32(&mut out, record.binding_start);
        put_u32(&mut out, record.binding_count);
        put_u32(&mut out, record.pipeline_blend.to_u32());
        put_u32(&mut out, record.depth_test.to_u32());
        put_bool(&mut out, record.depth_write);
        put_u32(&mut out, record.cull_mode.to_u32());
    }
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "render binding count")?,
    );
    for record in bindings {
        put_u32(&mut out, record.kind.to_u32());
        put_u32(&mut out, record.slot);
        put_u32(&mut out, record.target.to_u32());
        put_string_id(&mut out, record.name);
    }
    put_u32(
        &mut out,
        checked_u32(unsupported.len(), "unsupported boundary count")?,
    );
    for record in unsupported {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.pass_index);
        put_string_id(&mut out, record.feature);
        put_string_id(&mut out, record.expected_subsystem);
        put_string_id(&mut out, record.containment);
    }
    Ok(out)
}

type RenderGraphDecode = (
    Vec<SceneRenderGraphRecord>,
    Vec<SceneRenderPassRecord>,
    Vec<SceneRenderBindingRecord>,
    Vec<SceneUnsupportedRecord>,
);

fn decode_render_graphs(data: &[u8]) -> Result<RenderGraphDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let graph_count = decoder.u32()? as usize;
    let mut graphs = Vec::with_capacity(graph_count);
    for _ in 0..graph_count {
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(decoder.u32()?),
            pass_start: decoder.u32()?,
            pass_count: decoder.u32()?,
            unsupported_start: decoder.u32()?,
            unsupported_count: decoder.u32()?,
        });
    }
    let pass_count = decoder.u32()? as usize;
    let mut passes = Vec::with_capacity(pass_count);
    for _ in 0..pass_count {
        let id = decoder.u32()?;
        let role_raw = decoder.u32()?;
        let role = SceneRenderPassKind::from_u32(role_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render pass role", role_raw),
        )?;
        let object = SceneObjectHandle(decoder.u32()?);
        let pass_index = decoder.u32()?;
        let shader_key = decoder.string_id()?;
        let target_raw = decoder.u32()?;
        let target = SceneRenderTargetKind::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render target", target_raw),
        )?;
        passes.push(SceneRenderPassRecord {
            id,
            role,
            object,
            pass_index,
            shader_key,
            target,
            target_name: decoder.string_id()?,
            binding_start: decoder.u32()?,
            binding_count: decoder.u32()?,
            pipeline_blend: {
                let value = decoder.u32()?;
                ScenePipelineBlend::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("pipeline blend", value))?
            },
            depth_test: {
                let value = decoder.u32()?;
                SceneDepthTest::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("depth test", value))?
            },
            depth_write: decoder.bool()?,
            cull_mode: {
                let value = decoder.u32()?;
                SceneCullMode::from_u32(value)
                    .ok_or(SceneBinaryError::InvalidChunkValue("cull mode", value))?
            },
        });
    }
    let binding_count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(binding_count);
    for _ in 0..binding_count {
        let kind_raw = decoder.u32()?;
        let kind = SceneRenderBindingKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render binding kind", kind_raw),
        )?;
        let slot = decoder.u32()?;
        let target_raw = decoder.u32()?;
        let target = SceneRenderTargetKind::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("render binding target", target_raw),
        )?;
        bindings.push(SceneRenderBindingRecord {
            kind,
            slot,
            target,
            name: decoder.string_id()?,
        });
    }
    let unsupported_count = decoder.u32()? as usize;
    let mut unsupported = Vec::with_capacity(unsupported_count);
    for _ in 0..unsupported_count {
        unsupported.push(SceneUnsupportedRecord {
            object: SceneObjectHandle(decoder.u32()?),
            pass_index: decoder.u32()?,
            feature: decoder.string_id()?,
            expected_subsystem: decoder.string_id()?,
            containment: decoder.string_id()?,
        });
    }
    Ok((graphs, passes, bindings, unsupported))
}

fn encode_image_targets(targets: &[SceneImageTargetRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, targets.len() as u32);
    for record in targets {
        put_string_id(&mut out, record.name);
        put_u32(&mut out, record.role.to_u32());
        put_string_id(&mut out, record.format);
        put_u32(&mut out, record.scale_x_milli);
        put_u32(&mut out, record.scale_y_milli);
    }
    out
}

fn decode_image_targets(data: &[u8]) -> Result<Vec<SceneImageTargetRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let name = decoder.string_id()?;
        let role_raw = decoder.u32()?;
        let role = SceneRenderTargetKind::from_u32(role_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("image target role", role_raw),
        )?;
        records.push(SceneImageTargetRecord {
            name,
            role,
            format: decoder.string_id()?,
            scale_x_milli: decoder.u32()?,
            scale_y_milli: decoder.u32()?,
        });
    }
    Ok(records)
}

fn encode_shader_contracts(
    contracts: &[SceneShaderContractRecord],
    constant_names: &[SceneStringId],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(contracts.len(), "shader contract count")?,
    );
    for record in contracts {
        put_string_id(&mut out, record.shader_key);
        put_string_id(&mut out, record.pipeline_key);
        put_u32(&mut out, record.texture_slot_mask);
        put_u32(&mut out, record.constant_start);
        put_u32(&mut out, record.constant_count);
        put_u32(&mut out, record.resource_heap_count);
        put_u32(&mut out, record.sampler_heap_count);
    }
    put_u32(
        &mut out,
        checked_u32(constant_names.len(), "shader constant name count")?,
    );
    for name in constant_names {
        put_string_id(&mut out, *name);
    }
    Ok(out)
}

fn decode_shader_contracts(
    data: &[u8],
) -> Result<(Vec<SceneShaderContractRecord>, Vec<SceneStringId>), SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let contract_count = decoder.u32()? as usize;
    let mut contracts = Vec::with_capacity(contract_count);
    for _ in 0..contract_count {
        contracts.push(SceneShaderContractRecord {
            shader_key: decoder.string_id()?,
            pipeline_key: decoder.string_id()?,
            texture_slot_mask: decoder.u32()?,
            constant_start: decoder.u32()?,
            constant_count: decoder.u32()?,
            resource_heap_count: decoder.u32()?,
            sampler_heap_count: decoder.u32()?,
        });
    }
    let constant_count = decoder.u32()? as usize;
    let mut constant_names = Vec::with_capacity(constant_count);
    for _ in 0..constant_count {
        constant_names.push(decoder.string_id()?);
    }
    Ok((contracts, constant_names))
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bool(out: &mut Vec<u8>, value: bool) {
    out.push(u8::from(value));
}

fn put_string_id(out: &mut Vec<u8>, value: SceneStringId) {
    put_u32(out, value.0);
}

fn put_resource_id(out: &mut Vec<u8>, value: SceneResourceId) {
    put_u32(out, value.0);
}

fn put_vec3(out: &mut Vec<u8>, value: SceneVec3) {
    put_f32(out, value.x);
    put_f32(out, value.y);
    put_f32(out, value.z);
}

fn put_f32_array(out: &mut Vec<u8>, values: &[f32; 4]) {
    for value in values {
        put_f32(out, *value);
    }
}

fn read_u32_at(data: &[u8], offset: usize, name: &'static str) -> Result<u32, SceneBinaryError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::Truncated(name))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("u32 slice")))
}

fn read_u64_at(data: &[u8], offset: usize, name: &'static str) -> Result<u64, SceneBinaryError> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or(SceneBinaryError::Truncated(name))?;
    Ok(u64::from_le_bytes(bytes.try_into().expect("u64 slice")))
}

fn checked_u32(value: usize, name: &'static str) -> Result<u32, SceneBinaryError> {
    u32::try_from(value).map_err(|_| SceneBinaryError::SizeOverflow(name))
}

fn checked_u64(value: usize, name: &'static str) -> Result<u64, SceneBinaryError> {
    u64::try_from(value).map_err(|_| SceneBinaryError::SizeOverflow(name))
}

struct Decoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], SceneBinaryError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(SceneBinaryError::SizeOverflow("decode offset"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(SceneBinaryError::Truncated("chunk payload"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, SceneBinaryError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("u32 slice"),
        ))
    }

    fn i32(&mut self) -> Result<i32, SceneBinaryError> {
        Ok(i32::from_le_bytes(
            self.bytes(4)?.try_into().expect("i32 slice"),
        ))
    }

    fn i64(&mut self) -> Result<i64, SceneBinaryError> {
        Ok(i64::from_le_bytes(
            self.bytes(8)?.try_into().expect("i64 slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, SceneBinaryError> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?.try_into().expect("u64 slice"),
        ))
    }

    fn f32(&mut self) -> Result<f32, SceneBinaryError> {
        Ok(f32::from_le_bytes(
            self.bytes(4)?.try_into().expect("f32 slice"),
        ))
    }

    fn bool(&mut self) -> Result<bool, SceneBinaryError> {
        Ok(*self
            .bytes(1)?
            .first()
            .ok_or(SceneBinaryError::Truncated("bool"))?
            != 0)
    }

    fn string_id(&mut self) -> Result<SceneStringId, SceneBinaryError> {
        Ok(SceneStringId(self.u32()?))
    }

    fn resource_id(&mut self) -> Result<SceneResourceId, SceneBinaryError> {
        Ok(SceneResourceId(self.u32()?))
    }

    fn vec3(&mut self) -> Result<SceneVec3, SceneBinaryError> {
        Ok(SceneVec3 {
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    fn f32_array4(&mut self) -> Result<[f32; 4], SceneBinaryError> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_binary_round_trip_keeps_chunked_payloads_and_handles() {
        let mut document = SceneBinaryDocument {
            strings: vec![
                "title".to_owned(),
                "scene".to_owned(),
                "scene.json".to_owned(),
                "models/a.json".to_owned(),
                "loose".to_owned(),
            ],
            project: SceneProjectRecord {
                title: SceneStringId(0),
                wallpaper_type: SceneStringId(1),
                scene_file: SceneStringId(2),
                logical_width: 1920,
                logical_height: 1080,
                ..empty_project_record()
            },
            resource_payload: vec![1, 2, 3, 4],
            ..SceneBinaryDocument::default()
        };
        document.resources.push(SceneResourceRecord {
            id: SceneResourceId(0),
            kind: SceneResourceKind::ModelJson,
            path: SceneStringId(3),
            source: SceneStringId(4),
            payload_offset: 0,
            payload_len: 4,
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
        document.mesh_vertices.extend([
            SceneMeshVertexRecord {
                position: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                uv: [0.0, 1.0],
            },
            SceneMeshVertexRecord {
                position: SceneVec3 {
                    x: 32.0,
                    y: -16.0,
                    z: 0.0,
                },
                uv: [1.0, 1.0],
            },
            SceneMeshVertexRecord {
                position: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
                uv: [1.0, 0.0],
            },
            SceneMeshVertexRecord {
                position: SceneVec3 {
                    x: -32.0,
                    y: 16.0,
                    z: 0.0,
                },
                uv: [0.0, 0.0],
            },
        ]);
        document.mesh_indices.extend([0, 1, 2, 0, 2, 3]);

        let mut bytes = Vec::new();
        write_scene_binary(&document, &mut bytes).expect("write scene binary");
        let decoded = read_scene_binary_bytes(&bytes).expect("read scene binary");

        assert_eq!(
            decoded.feature_flags & SCENE_FEATURE_DESCRIPTOR_HEAP,
            SCENE_FEATURE_DESCRIPTOR_HEAP
        );
        assert_eq!(decoded.strings[0], "title");
        assert_eq!(decoded.project.logical_width, 1920);
        assert_eq!(decoded.resources[0].payload_len, 4);
        assert_eq!(decoded.resource_payload, vec![1, 2, 3, 4]);
        assert_eq!(decoded.meshes[0].width, 64.0);
        assert_eq!(decoded.mesh_vertices.len(), 4);
        assert_eq!(decoded.mesh_indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn scene_binary_rejects_chunk_table_item_count_mismatch() {
        let document = SceneBinaryDocument {
            strings: vec!["scene".to_owned()],
            ..SceneBinaryDocument::default()
        };
        let mut bytes = Vec::new();
        write_scene_binary(&document, &mut bytes).expect("write scene binary");
        let string_chunk_item_count_offset = HEADER_LEN + 24;
        bytes[string_chunk_item_count_offset..string_chunk_item_count_offset + 4]
            .copy_from_slice(&2u32.to_le_bytes());

        let err = read_scene_binary_bytes(&bytes).expect_err("count mismatch");

        assert!(matches!(
            err,
            SceneBinaryError::CountMismatch {
                chunk: "string table",
                expected: 2,
                actual: 1,
            }
        ));
    }
}
