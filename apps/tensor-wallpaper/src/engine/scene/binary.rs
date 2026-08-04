//! New Tensor Wallpaper scene engine binary reader/writer.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/project-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-pkg-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/material-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/effect-format.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device_graph.*`
//! - `references/tensor-wallpaper/godot/servers/rendering/storage/*`

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};

use super::abi::*;

mod codec;
mod document;
mod dynamic_text;
mod mesh_clipping;
mod particle;
mod pointer_binding;
mod scene_chunks;
mod script_binding;
mod shader_program;
mod texture;
mod timeline;
mod user_property_binding;

use codec::*;
use dynamic_text::{decode_dynamic_text, encode_dynamic_text};
use particle::{decode_particles, encode_particles};
use pointer_binding::{decode_pointer_bindings, encode_pointer_bindings};
use scene_chunks::*;
use script_binding::{decode_script_bindings, encode_script_bindings};
use shader_program::{DecodedShaderPrograms, decode_shader_programs, encode_shader_programs};

pub use document::SceneBinaryDocument;
#[cfg(test)]
use document::empty_project_record;
use texture::{decode_texture_mips, decode_textures, encode_texture_mips, encode_textures};
use timeline::{SceneTimelineRecords, decode_timelines, encode_timelines};
use user_property_binding::{decode_user_property_bindings, encode_user_property_bindings};

const HEADER_LEN: usize = 36;
const CHUNK_ENTRY_LEN: usize = 32;

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
    if !(SCENE_BINARY_MIN_READ_VERSION..=SCENE_BINARY_VERSION).contains(&version) {
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
    let (textures, texture_sequence_frames) =
        decode_textures(chunk_payload(&chunks, CHUNK_TEXTURE)?)?;
    ensure_chunk_count(&chunks, CHUNK_TEXTURE, "textures", textures.len())?;
    let texture_mips = decode_texture_mips(chunk_payload(&chunks, CHUNK_TEXTURE_MIP)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_TEXTURE_MIP,
        "texture mips",
        texture_mips.len(),
    )?;
    let texture_payload = chunk_payload(&chunks, CHUNK_TEXTURE_PAYLOAD)?.to_vec();
    ensure_chunk_count(
        &chunks,
        CHUNK_TEXTURE_PAYLOAD,
        "texture payload owners",
        textures.len(),
    )?;
    let (objects, object_effects) =
        decode_scene_objects(chunk_payload(&chunks, CHUNK_SCENE_OBJECT)?)?;
    ensure_chunk_count(&chunks, CHUNK_SCENE_OBJECT, "scene objects", objects.len())?;
    let (materials, material_passes, material_textures, material_constants) =
        decode_materials(chunk_payload(&chunks, CHUNK_MATERIAL)?)?;
    ensure_chunk_count(&chunks, CHUNK_MATERIAL, "materials", materials.len())?;
    let (
        meshes,
        mesh_vertices,
        mesh_indices,
        mesh_source_records,
        mesh_clipping_subdraws,
        mesh_clipping_source_ordinals,
        mesh_clipping_slices,
    ) = decode_meshes(chunk_payload(&chunks, CHUNK_MESH)?)?;
    ensure_chunk_count(&chunks, CHUNK_MESH, "mesh", meshes.len())?;
    let (puppets, puppet_bones, puppet_attachments) =
        decode_puppets(chunk_payload(&chunks, CHUNK_PUPPET)?)?;
    ensure_chunk_count(&chunks, CHUNK_PUPPET, "puppet", puppets.len())?;
    let particles = decode_particles(chunk_payload(&chunks, CHUNK_PARTICLE)?)?;
    ensure_chunk_count(&chunks, CHUNK_PARTICLE, "particle", particles.len())?;
    let (effects, effect_passes, effect_bindings, effect_combos, effect_fbos) =
        decode_effects(chunk_payload(&chunks, CHUNK_EFFECT)?)?;
    ensure_chunk_count(&chunks, CHUNK_EFFECT, "effects", effects.len())?;
    let SceneTimelineRecords {
        object_animation_layers,
        puppet_animation_clips,
        puppet_animation_tracks,
        puppet_animation_transform_samples,
        puppet_animation_opacity_samples,
        object_transform_tracks,
        object_transform_channels,
        object_transform_keyframes,
    } = decode_timelines(chunk_payload(&chunks, CHUNK_TIMELINE)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_TIMELINE,
        "timeline animation layers",
        object_animation_layers.len()
            + puppet_animation_clips.len()
            + object_transform_tracks.len(),
    )?;
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
    let DecodedShaderPrograms {
        programs: shader_programs,
        bindings: shader_bindings,
        stage_io: shader_stage_io,
        uniform_buffers: shader_uniform_buffers,
        uniform_members: shader_uniform_members,
        spirv: shader_spirv,
    } = decode_shader_programs(chunk_payload(&chunks, CHUNK_SHADER_PROGRAM)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_SHADER_PROGRAM,
        "shader programs",
        shader_programs.len(),
    )?;
    let script_programs = decode_script_bindings(chunk_payload(&chunks, CHUNK_SCRIPT_BINDING)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_SCRIPT_BINDING,
        "script binding",
        script_programs.len(),
    )?;
    let (dynamic_texts, dynamic_text_glyphs) =
        decode_dynamic_text(chunk_payload(&chunks, CHUNK_DYNAMIC_TEXT)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_DYNAMIC_TEXT,
        "dynamic text",
        dynamic_texts.len(),
    )?;
    let user_property_bindings =
        decode_user_property_bindings(chunk_payload(&chunks, CHUNK_USER_PROPERTY_BINDING)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_USER_PROPERTY_BINDING,
        "user property binding",
        user_property_bindings.len(),
    )?;
    ensure_chunk_count(&chunks, CHUNK_AUDIO, "audio", 0)?;
    let (camera_parallax, object_parallax_depths) =
        decode_pointer_bindings(chunk_payload(&chunks, CHUNK_POINTER_BINDING)?)?;
    ensure_chunk_count(
        &chunks,
        CHUNK_POINTER_BINDING,
        "pointer parallax bindings",
        object_parallax_depths.len(),
    )?;

    Ok(SceneBinaryDocument {
        feature_flags,
        strings,
        project,
        resources,
        resource_payload,
        textures,
        texture_mips,
        texture_sequence_frames,
        texture_payload,
        objects,
        object_effects,
        object_animation_layers,
        object_transform_tracks,
        object_transform_channels,
        object_transform_keyframes,
        puppet_animation_clips,
        puppet_animation_tracks,
        puppet_animation_transform_samples,
        puppet_animation_opacity_samples,
        materials,
        material_passes,
        material_textures,
        material_constants,
        meshes,
        mesh_vertices,
        mesh_indices,
        mesh_source_records,
        mesh_clipping_subdraws,
        mesh_clipping_source_ordinals,
        mesh_clipping_slices,
        puppets,
        puppet_bones,
        puppet_attachments,
        particles,
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
        shader_programs,
        shader_bindings,
        shader_stage_io,
        shader_uniform_buffers,
        shader_uniform_members,
        shader_spirv,
        script_programs,
        dynamic_texts,
        dynamic_text_glyphs,
        user_property_bindings,
        camera_parallax,
        object_parallax_depths,
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
            Self::InvalidMagic => f.write_str("invalid Tensor Wallpaper scene binary magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported Tensor Wallpaper scene binary version {version}")
            }
            Self::UnsupportedEndianness(value) => {
                write!(f, "unsupported Tensor Wallpaper scene binary endianness tag {value}")
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
            data: encode_textures(&document.textures, &document.texture_sequence_frames),
        },
        SceneEncodedChunk {
            kind: CHUNK_TEXTURE_MIP,
            item_count: checked_u32(document.texture_mips.len(), "texture mip count")?,
            data: encode_texture_mips(&document.texture_mips),
        },
        SceneEncodedChunk {
            kind: CHUNK_TEXTURE_PAYLOAD,
            item_count: checked_u32(document.textures.len(), "texture payload owner count")?,
            data: document.texture_payload.clone(),
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
                &document.mesh_source_records,
                &document.mesh_clipping_subdraws,
                &document.mesh_clipping_source_ordinals,
                &document.mesh_clipping_slices,
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
        SceneEncodedChunk {
            kind: CHUNK_TIMELINE,
            item_count: checked_u32(
                document
                    .object_animation_layers
                    .len()
                    .saturating_add(document.puppet_animation_clips.len())
                    .saturating_add(document.object_transform_tracks.len()),
                "timeline primary record count",
            )?,
            data: encode_timelines(
                &document.object_animation_layers,
                &document.puppet_animation_clips,
                &document.puppet_animation_tracks,
                &document.puppet_animation_transform_samples,
                &document.puppet_animation_opacity_samples,
                &document.object_transform_tracks,
                &document.object_transform_channels,
                &document.object_transform_keyframes,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_PUPPET,
            item_count: checked_u32(document.puppets.len(), "puppet count")?,
            data: encode_puppets(
                &document.puppets,
                &document.puppet_bones,
                &document.puppet_attachments,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_PARTICLE,
            item_count: checked_u32(document.particles.len(), "particle count")?,
            data: encode_particles(&document.particles)?,
        },
        empty_count_chunk(CHUNK_AUDIO),
        SceneEncodedChunk {
            kind: CHUNK_SCRIPT_BINDING,
            item_count: checked_u32(document.script_programs.len(), "script binding count")?,
            data: encode_script_bindings(&document.script_programs)?,
        },
        SceneEncodedChunk {
            kind: CHUNK_DYNAMIC_TEXT,
            item_count: checked_u32(document.dynamic_texts.len(), "dynamic text count")?,
            data: encode_dynamic_text(&document.dynamic_texts, &document.dynamic_text_glyphs)?,
        },
        SceneEncodedChunk {
            kind: CHUNK_POINTER_BINDING,
            item_count: checked_u32(
                document.object_parallax_depths.len(),
                "object parallax depth count",
            )?,
            data: encode_pointer_bindings(
                document.camera_parallax,
                &document.object_parallax_depths,
            )?,
        },
        SceneEncodedChunk {
            kind: CHUNK_USER_PROPERTY_BINDING,
            item_count: checked_u32(
                document.user_property_bindings.len(),
                "user property binding count",
            )?,
            data: encode_user_property_bindings(&document.user_property_bindings)?,
        },
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
        SceneEncodedChunk {
            kind: CHUNK_SHADER_PROGRAM,
            item_count: checked_u32(document.shader_programs.len(), "shader program count")?,
            data: encode_shader_programs(
                &document.shader_programs,
                &document.shader_bindings,
                &document.shader_stage_io,
                &document.shader_uniform_buffers,
                &document.shader_uniform_members,
                &document.shader_spirv,
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

#[cfg(test)]
mod tests;
