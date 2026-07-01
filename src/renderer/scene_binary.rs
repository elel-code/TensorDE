use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::core::scene::binary::{
    SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE, SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
    SCENE_BINARY_EFFECT_PASS_RECORD_SIZE, SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
    SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH, SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES,
    SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, SCENE_BINARY_HEADER_SIZE,
    SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, SCENE_BINARY_NODE_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE,
    SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS, SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SCENE_BINARY_PUPPET_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
    SCENE_BINARY_RENDER_STATE_RECORD_SIZE, SCENE_BINARY_RESOURCE_RECORD_SIZE,
    SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryEffectPassRecord,
    SceneBinaryEffectUvTransformRecord, SceneBinaryError, SceneBinaryGeometryRecord,
    SceneBinaryLayoutPlan, SceneBinaryMaterialPassRecord, SceneBinaryParticleEmitterRecord,
    SceneBinaryResourceRecord, SceneBinaryTextureSlotRecord, decode_debug_name_record,
    decode_effect_pass_record, decode_effect_uv_transform_record, decode_geometry_index_record,
    decode_geometry_record, decode_geometry_vertex_record, decode_material_pass_record,
    decode_node_record, decode_particle_emitter_record, decode_puppet_clip_record,
    decode_puppet_frame_record, decode_puppet_layer_record, decode_puppet_record,
    decode_puppet_skin_bone_record, decode_puppet_skin_vertex_record, decode_render_state_record,
    decode_resource_record, decode_scene_binary_header_table, decode_texture_slot_record,
    decode_transform_keyframe_record, decode_transform_timeline_record,
    scene_binary_particle_shape_kind, scene_binary_particle_transform,
};
use crate::core::scene::{
    SceneEffectUvExtent, SceneEffectUvMapping, SceneEffectUvTransform, SceneMesh, SceneMeshSkin,
    SceneMeshSkinBone, SceneMeshSkinVertex, SceneMeshVertex, ScenePuppetAnimationBone,
    ScenePuppetAnimationClip, ScenePuppetAnimationLayer,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneSystems,
    SceneTextAlign, SceneTextureRegion, SceneTransform,
};
use crate::renderer::{
    RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderLayer,
    SceneRenderTextureSlot, SceneWallpaperPlan, SceneWallpaperRuntimeFrame,
};

const BINARY_TRANSFORM_PROPERTY_DEFAULT: u16 = 0;
const BINARY_TRANSFORM_PROPERTY_X: u16 = 1;
const BINARY_TRANSFORM_PROPERTY_Y: u16 = 2;
const BINARY_TRANSFORM_PROPERTY_SCALE_X: u16 = 3;
const BINARY_TRANSFORM_PROPERTY_SCALE_Y: u16 = 4;
const BINARY_TRANSFORM_PROPERTY_OPACITY: u16 = 5;
const BINARY_TRANSFORM_PROPERTY_ROTATION_DEG: u16 = 6;
const BINARY_TRANSFORM_PROPERTY_WIDTH: u16 = 7;
const BINARY_TRANSFORM_PROPERTY_HEIGHT: u16 = 8;
const BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS: u16 = 9;
const BINARY_TRANSFORM_FLAG_LOOP: u16 = 1;
const BINARY_NODE_FLAG_VISIBLE: u16 = 1;
const BINARY_NODE_FLAG_COLOR: u16 = 1 << 7;
const BINARY_NODE_FLAG_STROKE_COLOR: u16 = 1 << 8;
const BINARY_NODE_FLAG_STROKE_WIDTH: u16 = 1 << 9;
const BINARY_NODE_FLAG_CORNER_RADIUS: u16 = 1 << 10;
const BINARY_EFFECT_UV_HAS_INPUT_EXTENT: u16 = 1;
const BINARY_EFFECT_UV_HAS_MASK_EXTENT: u16 = 1 << 1;
const BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT: u16 = 1 << 2;
const BINARY_TEXTURE_ROLE_BASE_COLOR: u16 = 1;

#[derive(Debug, Clone)]
struct BinarySceneResource {
    id_name: u32,
    source: Option<PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
}

struct BinarySceneReader {
    file: File,
    file_len: usize,
    layout: SceneBinaryLayoutPlan,
}

impl BinarySceneReader {
    fn open(path: &Path) -> Result<Self, RendererPlanError> {
        let mut file = File::open(path).map_err(|err| {
            RendererPlanError::PackageLoad(format!(
                "failed to open binary scene {}: {err}",
                path.display()
            ))
        })?;
        let file_len = usize::try_from(
            file.metadata()
                .map_err(|err| {
                    RendererPlanError::PackageLoad(format!(
                        "failed to stat binary scene {}: {err}",
                        path.display()
                    ))
                })?
                .len(),
        )
        .map_err(|_| {
            RendererPlanError::PackageLoad(format!(
                "binary scene {} is too large to address",
                path.display()
            ))
        })?;
        let header = binary_scene_read_exact_at(&mut file, 0, SCENE_BINARY_HEADER_SIZE)?;
        let chunk_count = binary_scene_read_u32(&header, 12).map_err(binary_plan_error)?;
        let chunk_table_offset = binary_scene_read_u64(&header, 16).map_err(binary_plan_error)?;
        let table_start = usize::try_from(chunk_table_offset).map_err(|_| {
            binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: file_len,
            })
        })?;
        let table_size = usize::try_from(chunk_count)
            .ok()
            .and_then(|count| count.checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE))
            .ok_or_else(|| {
                binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                    offset: chunk_table_offset,
                    count: chunk_count,
                    container_len: file_len,
                })
            })?;
        let header_table_len = table_start.checked_add(table_size).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: file_len,
            })
        })?;
        let header_table = if header_table_len == SCENE_BINARY_HEADER_SIZE {
            header
        } else {
            binary_scene_read_exact_at(&mut file, 0, header_table_len)?
        };
        let layout =
            decode_scene_binary_header_table(&header_table, file_len).map_err(binary_plan_error)?;
        Ok(Self {
            file,
            file_len,
            layout,
        })
    }

    fn chunk_count(&self, kind: SceneBinaryChunkKind) -> usize {
        self.layout
            .chunk(kind)
            .map_or(0, |chunk| chunk.record_count as usize)
    }

    fn chunk_payload(&mut self, kind: SceneBinaryChunkKind) -> Result<Vec<u8>, RendererPlanError> {
        let descriptor = self
            .layout
            .chunk(kind)
            .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
        let length = usize::try_from(descriptor.length).map_err(|_| {
            binary_plan_error(SceneBinaryError::ChunkOutOfBounds {
                kind,
                offset: descriptor.offset,
                length: descriptor.length,
                container_len: self.file_len,
            })
        })?;
        binary_scene_read_exact_at(&mut self.file, descriptor.offset, length)
    }

    fn records<T>(
        &mut self,
        kind: SceneBinaryChunkKind,
        record_size: usize,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<Vec<T>, RendererPlanError> {
        let descriptor = self
            .layout
            .chunk(kind)
            .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
        self.record_range(kind, record_size, 0, descriptor.record_count, decode)
    }

    fn record_at<T>(
        &mut self,
        kind: SceneBinaryChunkKind,
        record_size: usize,
        record_index: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<T, RendererPlanError> {
        let mut records = self.record_range(kind, record_size, record_index, 1, decode)?;
        records.pop().ok_or_else(|| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record: record_index,
                record_count: 1,
                chunk_record_count: self
                    .layout
                    .chunk(kind)
                    .map_or(0, |chunk| chunk.record_count),
            })
        })
    }

    fn record_range<T>(
        &mut self,
        kind: SceneBinaryChunkKind,
        record_size: usize,
        first_record: u32,
        record_count: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<Vec<T>, RendererPlanError> {
        let descriptor = self
            .layout
            .chunk(kind)
            .cloned()
            .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
        if record_count == 0 {
            return Ok(Vec::new());
        }
        let first = usize::try_from(first_record).map_err(|_| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })
        })?;
        let count = usize::try_from(record_count).map_err(|_| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })
        })?;
        let end_record = first.checked_add(count).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })
        })?;
        if end_record > descriptor.record_count as usize {
            return Err(binary_plan_error(
                SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                },
            ));
        }
        let byte_offset = first.checked_mul(record_size).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })
        })?;
        let byte_len = count.checked_mul(record_size).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })
        })?;
        let file_offset = descriptor
            .offset
            .checked_add(byte_offset as u64)
            .ok_or_else(|| {
                binary_plan_error(SceneBinaryError::ChunkOutOfBounds {
                    kind,
                    offset: descriptor.offset,
                    length: descriptor.length,
                    container_len: self.file_len,
                })
            })?;
        let bytes = binary_scene_read_exact_at(&mut self.file, file_offset, byte_len)?;
        let mut records = Vec::with_capacity(count);
        for chunk in bytes.chunks_exact(record_size) {
            records.push(decode(chunk).map_err(binary_plan_error)?);
        }
        Ok(records)
    }
}

pub(crate) struct SceneBinaryRuntimeSampler {
    reader: BinarySceneReader,
    names: BinarySceneNames,
    resources: Vec<BinarySceneResource>,
    package_root: PathBuf,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers_scratch: Vec<SceneRenderLayer>,
}

impl SceneBinaryRuntimeSampler {
    pub(crate) fn from_plan(plan: &SceneWallpaperPlan) -> Result<Option<Self>, RendererPlanError> {
        let Some(source_path) = plan.source.as_ref() else {
            return Ok(None);
        };
        if !scene_binary_source_is_gscn(source_path) {
            return Ok(None);
        }
        let mut reader = BinarySceneReader::open(source_path)?;
        let names = binary_scene_names(&mut reader)?;
        let package_root = binary_scene_package_root(source_path);
        let resources = binary_scene_resources(&mut reader, &names, &package_root)?;
        let scene_size = binary_scene_size(&mut reader)?;
        Ok(Some(Self {
            reader,
            names,
            resources,
            package_root,
            scene_size,
            scene_fit: plan.scene_fit,
            layers_scratch: Vec::new(),
        }))
    }

    pub(crate) fn sample_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        binary_scene_render_layers_into(
            &mut self.reader,
            &self.names,
            &self.resources,
            time_ms,
            &mut self.layers_scratch,
        )?;
        Ok(SceneWallpaperRuntimeFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.scene_size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.layers_scratch),
        })
    }

    pub(crate) fn recycle_frame(&mut self, mut frame: SceneWallpaperRuntimeFrame) {
        frame.layers.clear();
        self.layers_scratch = frame.layers;
    }

    pub(crate) fn package_root(&self) -> &Path {
        &self.package_root
    }
}

fn scene_binary_source_is_gscn(source_path: &Path) -> bool {
    source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gscn"))
}

#[derive(Debug, Clone)]
struct BinarySceneNames {
    names: Vec<Option<String>>,
}

impl BinarySceneNames {
    fn name(&self, id: u32) -> Option<&str> {
        if id == SCENE_BINARY_NONE_ID {
            return None;
        }
        self.names
            .get(id as usize)
            .and_then(|value| value.as_deref())
    }
}

pub(super) fn scene_wallpaper_plan_from_gscn_path(
    output_name: String,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let mut reader = BinarySceneReader::open(&source_path)?;
    let names = binary_scene_names(&mut reader)?;
    let package_root = binary_scene_package_root(&source_path);
    let resources = binary_scene_resources(&mut reader, &names, &package_root)?;
    let scene_size = binary_scene_size(&mut reader)?;
    let layers = binary_scene_render_layers(&mut reader, &names, &resources, snapshot_time_ms)?;
    let (timeline_animation_count, timeline_animated_layer_count) =
        binary_scene_timeline_counts(&mut reader)?;
    let puppet_animation_layer_count = binary_scene_puppet_animation_layer_count(&mut reader)?;
    let particle_emitter_count = reader.chunk_count(SceneBinaryChunkKind::ParticleEmitter);
    let scene_systems = SceneSystems {
        particles: if particle_emitter_count > 0 {
            crate::core::SceneSystemStatus::Ready
        } else {
            crate::core::SceneSystemStatus::Absent
        },
        ..Default::default()
    };

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps: None,
        target_max_fps,
        snapshot_time_ms,
        scene_size,
        scene_fit: fit_override.unwrap_or(FitMode::Cover),
        scene_systems,
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count,
        timeline_animated_layer_count,
        puppet_animation_layer_count,
        property_binding_count: 0,
        cursor_parallax_input_ready: false,
        scene_input_properties: BTreeMap::new(),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: reader.chunk_count(SceneBinaryChunkKind::MaterialPass),
        scene_material_graph_resource_count: resources.len(),
        scene_effect_graph_count: reader.chunk_count(SceneBinaryChunkKind::EffectPass),
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        display: None,
        layers,
    })
}

fn binary_scene_resources(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    package_root: &Path,
) -> Result<Vec<BinarySceneResource>, RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::ResourceTable,
        SCENE_BINARY_RESOURCE_RECORD_SIZE,
        decode_resource_record,
    )?;
    let mut resources = Vec::with_capacity(records.len());
    for record in records {
        resources.push(binary_scene_resource(record, names, package_root)?);
    }
    Ok(resources)
}

fn binary_scene_resource(
    record: SceneBinaryResourceRecord,
    names: &BinarySceneNames,
    package_root: &Path,
) -> Result<BinarySceneResource, RendererPlanError> {
    let source = binary_name(names, record.source_name)
        .map(|source| binary_scene_resource_path(package_root, source));
    Ok(BinarySceneResource {
        id_name: record.id_name,
        source,
        width: (record.width > 0).then_some(record.width),
        height: (record.height > 0).then_some(record.height),
    })
}

fn binary_scene_names(
    reader: &mut BinarySceneReader,
) -> Result<BinarySceneNames, RendererPlanError> {
    let descriptor = reader
        .layout
        .chunk(SceneBinaryChunkKind::DebugNames)
        .cloned()
        .ok_or_else(|| {
            binary_plan_error(SceneBinaryError::MissingChunk {
                kind: SceneBinaryChunkKind::DebugNames,
            })
        })?;
    let payload = reader.chunk_payload(SceneBinaryChunkKind::DebugNames)?;
    let record_bytes = usize::try_from(descriptor.record_count)
        .ok()
        .and_then(|count| count.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
        .ok_or_else(|| {
            binary_plan_error(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count: descriptor.record_count,
                length: payload.len(),
            })
        })?;
    if payload.len() < record_bytes {
        return Err(binary_plan_error(SceneBinaryError::InvalidRecordPayload {
            kind: SceneBinaryChunkKind::DebugNames,
            record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
            record_count: descriptor.record_count,
            length: payload.len(),
        }));
    }
    let (record_bytes, string_bytes) = payload.split_at(record_bytes);
    let mut names = Vec::<Option<String>>::new();
    for record in record_bytes.chunks_exact(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE) {
        let record = decode_debug_name_record(record).map_err(binary_plan_error)?;
        let start = usize::try_from(record.offset).map_err(|_| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let length = usize::try_from(record.length).map_err(|_| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let end = start.checked_add(length).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let Some(bytes) = string_bytes.get(start..end) else {
            return Err(binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            }));
        };
        let name = std::str::from_utf8(bytes)
            .map_err(|_| binary_plan_error(SceneBinaryError::InvalidNameUtf8 { id: record.id }))?;
        let id = record.id as usize;
        if names.len() <= id {
            names.resize_with(id + 1, || None);
        }
        names[id] = Some(name.to_owned());
    }
    Ok(BinarySceneNames { names })
}

fn binary_scene_size(
    reader: &mut BinarySceneReader,
) -> Result<Option<SceneSize>, RendererPlanError> {
    let render_state = reader
        .records(
            SceneBinaryChunkKind::RenderState,
            SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
            decode_render_state_record,
        )?
        .into_iter()
        .next();
    Ok(render_state.and_then(|state| {
        (state.width > 0 && state.height > 0).then_some(SceneSize {
            width: state.width,
            height: state.height,
        })
    }))
}

fn binary_scene_timeline_counts(
    reader: &mut BinarySceneReader,
) -> Result<(usize, usize), RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::TransformTimeline,
        SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
        decode_transform_timeline_record,
    )?;
    let mut channel_count = 0usize;
    let mut owner_names = BTreeSet::new();
    for record in records {
        if record.keyframe_count == 0 {
            continue;
        }
        channel_count = channel_count.saturating_add(1);
        owner_names.insert(record.owner_name);
    }
    Ok((channel_count, owner_names.len()))
}

fn binary_scene_puppet_animation_layer_count(
    reader: &mut BinarySceneReader,
) -> Result<usize, RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::Puppet,
        SCENE_BINARY_PUPPET_RECORD_SIZE,
        decode_puppet_record,
    )?;
    let mut count = 0usize;
    for record in records {
        count = count.saturating_add(record.animation_layer_count as usize);
    }
    Ok(count)
}

fn binary_scene_render_layers(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    snapshot_time_ms: u64,
) -> Result<Vec<SceneRenderLayer>, RendererPlanError> {
    let mut layers = Vec::new();
    binary_scene_render_layers_into(reader, names, resources, snapshot_time_ms, &mut layers)?;
    Ok(layers)
}

fn binary_scene_render_layers_into(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    snapshot_time_ms: u64,
    layers: &mut Vec<SceneRenderLayer>,
) -> Result<(), RendererPlanError> {
    layers.clear();
    let node_records = reader.records(
        SceneBinaryChunkKind::NodeTable,
        SCENE_BINARY_NODE_RECORD_SIZE,
        decode_node_record,
    )?;
    let mut node_geometries = Vec::with_capacity(node_records.len());
    let mut node_states = Vec::with_capacity(node_records.len());
    for node in &node_records {
        let geometry = if node.geometry_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(reader.record_at(
                SceneBinaryChunkKind::Geometry,
                SCENE_BINARY_GEOMETRY_RECORD_SIZE,
                node.geometry_index,
                decode_geometry_record,
            )?)
        };
        let local_state = binary_scene_node_state(reader, *node, geometry, snapshot_time_ms)?;
        let parent_state = binary_scene_parent_node_state(&node_states, node.parent_index)?;
        node_states.push(binary_scene_effective_node_state(
            *node,
            local_state,
            parent_state,
        ));
        node_geometries.push(geometry);
    }
    layers.reserve(node_records.len());
    for (node, (geometry, node_state)) in node_records
        .into_iter()
        .zip(node_geometries.into_iter().zip(node_states.into_iter()))
    {
        if !node_state.visible {
            continue;
        }
        let Some(geometry) = geometry else { continue };
        let Some(kind) = binary_scene_node_kind(node.kind) else {
            continue;
        };
        if !binary_scene_node_kind_is_renderable(kind) {
            continue;
        }
        let material = if node.material_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(reader.record_at(
                SceneBinaryChunkKind::MaterialPass,
                SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
                node.material_index,
                decode_material_pass_record,
            )?)
        };
        if kind == SceneNodeKind::ParticleEmitter
            || geometry.primitive_kind == SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES
        {
            if node.particle_index != SCENE_BINARY_NONE_ID {
                let particle = reader.record_at(
                    SceneBinaryChunkKind::ParticleEmitter,
                    SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
                    node.particle_index,
                    decode_particle_emitter_record,
                )?;
                binary_scene_particle_render_layers(
                    reader,
                    resources,
                    node,
                    particle,
                    material,
                    node_state.state,
                    snapshot_time_ms,
                    layers,
                )?;
            }
            continue;
        }
        let layer = binary_scene_render_layer(
            reader,
            names,
            resources,
            node,
            geometry,
            material,
            kind,
            node_state.state,
            snapshot_time_ms,
        )?;
        layers.push(layer);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn binary_scene_particle_render_layers(
    reader: &mut BinarySceneReader,
    resources: &[BinarySceneResource],
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    particle: SceneBinaryParticleEmitterRecord,
    material: Option<SceneBinaryMaterialPassRecord>,
    node_state: BinarySceneNodeState,
    snapshot_time_ms: u64,
    layers: &mut Vec<SceneRenderLayer>,
) -> Result<(), RendererPlanError> {
    let particle_count = particle.particle_count();
    if particle_count == 0 || node_state.opacity <= 0.0 {
        return Ok(());
    }

    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let source = node_resource.and_then(|resource| resource.source.clone());
    let texture_slots = if let Some(material) = material {
        let slots = binary_scene_material_texture_slots(reader, material, resources)?;
        if slots.is_empty() {
            binary_scene_particle_base_texture_slot(node_resource)
        } else {
            slots
        }
    } else {
        binary_scene_particle_base_texture_slot(node_resource)
    };
    let layer_kind = if source.is_some() {
        SceneNodeKind::Image
    } else {
        scene_binary_particle_shape_kind(particle.shape)
    };
    let blend_mode = material
        .map(|material| binary_scene_blend_mode(material.blend_mode))
        .unwrap_or_default();
    let color = Some(binary_scene_rgba_hex(particle.color_rgba));
    let (parent_sin, parent_cos) = node_state.transform.rotation_deg.to_radians().sin_cos();
    layers.reserve(particle_count as usize);
    for index in 0..particle_count {
        let Some((particle_opacity, x, y, rotation_deg)) =
            particle.opacity_and_transform_at(snapshot_time_ms, index)
        else {
            continue;
        };
        let opacity = node_state.opacity * particle_opacity;
        if opacity <= 0.0 {
            continue;
        }
        layers.push(SceneRenderLayer {
            id: String::new(),
            kind: layer_kind,
            source: source.clone(),
            texture_slots: texture_slots.clone(),
            alpha_texture_slot: None,
            alpha_texture_mode: SceneRenderAlphaTextureMode::Multiply,
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None::<SceneTextureRegion>,
            effect_motion: Default::default(),
            blend_mode,
            audio: Vec::new(),
            color: color.clone(),
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: Some(f64::from(particle.particle_width)),
            height: Some(f64::from(particle.particle_height)),
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: binary_scene_fit(node.fit),
            opacity: opacity.clamp(0.0, 1.0),
            transform: scene_binary_particle_transform(
                node_state.transform,
                parent_sin,
                parent_cos,
                x,
                y,
                rotation_deg,
            ),
        });
    }
    Ok(())
}

fn binary_scene_particle_base_texture_slot(
    resource: Option<&BinarySceneResource>,
) -> Vec<SceneRenderTextureSlot> {
    let Some(resource) = resource else {
        return Vec::new();
    };
    let Some(source) = resource.source.clone() else {
        return Vec::new();
    };
    vec![SceneRenderTextureSlot {
        slot: 0,
        source,
        width: resource.width,
        height: resource.height,
    }]
}

fn binary_scene_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    geometry: SceneBinaryGeometryRecord,
    material: Option<SceneBinaryMaterialPassRecord>,
    kind: SceneNodeKind,
    node_state: BinarySceneNodeState,
    snapshot_time_ms: u64,
) -> Result<SceneRenderLayer, RendererPlanError> {
    let material_texture_slots = if let Some(material) = material {
        binary_scene_material_texture_slots(reader, material, resources)?
    } else {
        Vec::new()
    };
    let image_effect_passes = if let Some(material) = material {
        binary_scene_image_effect_passes(reader, names, material, resources)?
    } else {
        Vec::new()
    };
    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let source = node_resource
        .and_then(|resource| resource.source.clone())
        .or_else(|| {
            material_texture_slots
                .iter()
                .find(|slot| slot.slot == 0)
                .map(|slot| slot.source.clone())
        });
    Ok(SceneRenderLayer {
        id: binary_name(names, node.id_name)
            .unwrap_or("binary-node")
            .to_owned(),
        kind,
        source,
        texture_slots: material_texture_slots,
        alpha_texture_slot: material.and_then(binary_scene_alpha_texture_slot),
        alpha_texture_mode: material
            .map(binary_scene_alpha_texture_mode)
            .unwrap_or_default(),
        image_effect_passes,
        composite_key: None,
        texture_region: None::<SceneTextureRegion>,
        effect_motion: Default::default(),
        blend_mode: material
            .map(|material| binary_scene_blend_mode(material.blend_mode))
            .unwrap_or_default(),
        audio: Vec::new(),
        color: binary_scene_flagged_color(node.flags, BINARY_NODE_FLAG_COLOR, node.color_rgba),
        stroke_color: binary_scene_flagged_color(
            node.flags,
            BINARY_NODE_FLAG_STROKE_COLOR,
            node.stroke_color_rgba,
        ),
        stroke_width: (node.flags & BINARY_NODE_FLAG_STROKE_WIDTH != 0)
            .then_some(f64::from(node.stroke_width)),
        corner_radius: node_state.corner_radius,
        width: node_state.width,
        height: node_state.height,
        mesh: binary_scene_mesh(reader, geometry, node.puppet_index, snapshot_time_ms)?,
        text: binary_name(names, node.text_name).map(str::to_owned),
        font_size: (node.font_size > 0.0).then_some(f64::from(node.font_size)),
        font_family: binary_name(names, node.font_family_name).map(str::to_owned),
        font_source: binary_resource_by_name(resources, node.font_resource_name)
            .and_then(|resource| resource.source.clone()),
        font_weight: binary_name(names, node.font_weight_name).map(str::to_owned),
        text_align: binary_scene_text_align(node.text_align),
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit: binary_scene_fit(node.fit),
        opacity: node_state.opacity,
        transform: node_state.transform,
    })
}

fn binary_scene_material_texture_slots(
    reader: &mut BinarySceneReader,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        material.first_texture_slot,
        material.texture_slot_count,
        decode_texture_slot_record,
    )?;
    binary_scene_texture_slots(slots, resources, |slot| {
        slot.role_flags & BINARY_TEXTURE_ROLE_BASE_COLOR != 0
    })
}

fn binary_scene_image_effect_passes(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderImageEffectPass>, RendererPlanError> {
    let passes = reader.record_range(
        SceneBinaryChunkKind::EffectPass,
        SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
        material.first_effect_pass,
        material.effect_pass_count,
        decode_effect_pass_record,
    )?;
    let mut output = Vec::with_capacity(passes.len());
    for pass in passes {
        output.push(binary_scene_image_effect_pass(
            reader, names, resources, pass,
        )?);
    }
    Ok(output)
}

fn binary_scene_image_effect_pass(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    pass: SceneBinaryEffectPassRecord,
) -> Result<SceneRenderImageEffectPass, RendererPlanError> {
    let texture_slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        pass.first_texture_slot,
        pass.texture_slot_count,
        decode_texture_slot_record,
    )?;
    let transforms = reader.record_range(
        SceneBinaryChunkKind::EffectUvTransform,
        SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
        pass.first_effect_uv_transform,
        pass.effect_uv_transform_count,
        decode_effect_uv_transform_record,
    )?;
    let effect_file = binary_name(names, pass.effect_name)
        .unwrap_or("")
        .to_owned();
    let shader = binary_name(names, pass.shader_name).map(str::to_owned);
    let blending = binary_name(names, pass.blending_name).map(str::to_owned);
    Ok(SceneRenderImageEffectPass {
        effect_file: effect_file.clone(),
        runtime: binary_scene_effect_runtime(pass.kind, &effect_file),
        pass_index: pass.pass_index as usize,
        shader,
        blending,
        depthtest: binary_scene_material_flag(pass.depth_test),
        depthwrite: binary_scene_material_flag(pass.depth_write),
        cullmode: binary_scene_cull_mode(pass.cull_mode),
        texture_slots: binary_scene_texture_slots(texture_slots, resources, |_| true)?,
        effect_uv_transform: transforms
            .into_iter()
            .next()
            .map(binary_scene_effect_uv_transform),
        combos: BTreeMap::new(),
        constant_shader_values: BTreeMap::<String, Value>::new(),
    })
}

fn binary_scene_texture_slots(
    slots: Vec<SceneBinaryTextureSlotRecord>,
    resources: &[BinarySceneResource],
    keep: impl Fn(&SceneBinaryTextureSlotRecord) -> bool,
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let mut output = Vec::with_capacity(slots.len());
    for slot in slots {
        if !keep(&slot) {
            continue;
        }
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        let Some(source) = resource.source.clone() else {
            continue;
        };
        output.push(SceneRenderTextureSlot {
            slot: slot.slot,
            source,
            width: resource.width.or((slot.width > 0).then_some(slot.width)),
            height: resource.height.or((slot.height > 0).then_some(slot.height)),
        });
    }
    Ok(output)
}

fn binary_scene_mesh(
    reader: &mut BinarySceneReader,
    geometry: SceneBinaryGeometryRecord,
    puppet_index: u32,
    snapshot_time_ms: u64,
) -> Result<Option<Arc<SceneMesh>>, RendererPlanError> {
    if geometry.primitive_kind != SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH
        || geometry.vertex_layout != SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY
    {
        return Ok(None);
    }
    let vertex_records = reader.record_range(
        SceneBinaryChunkKind::GeometryVertices,
        SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
        geometry.first_vertex,
        geometry.vertex_count,
        decode_geometry_vertex_record,
    )?;
    let index_records = reader.record_range(
        SceneBinaryChunkKind::GeometryIndices,
        SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
        geometry.first_index,
        geometry.index_count,
        decode_geometry_index_record,
    )?;
    let mut vertices = Vec::with_capacity(vertex_records.len());
    for vertex in vertex_records {
        vertices.push(SceneMeshVertex {
            x: f64::from(vertex.x),
            y: f64::from(vertex.y),
            u: f64::from(vertex.u),
            v: f64::from(vertex.v),
            opacity: f64::from(vertex.opacity),
        });
    }
    let mut indices = Vec::with_capacity(index_records.len());
    for index in index_records {
        indices.push(index.index);
    }
    let mut mesh = SceneMesh {
        vertices,
        indices,
        skin: None,
        puppet_clips: Vec::new(),
    };
    if puppet_index != SCENE_BINARY_NONE_ID {
        mesh = binary_scene_sampled_puppet_mesh(reader, mesh, puppet_index, snapshot_time_ms)?;
    }
    Ok(Some(Arc::new(mesh)))
}

fn binary_scene_sampled_puppet_mesh(
    reader: &mut BinarySceneReader,
    mut mesh: SceneMesh,
    puppet_index: u32,
    snapshot_time_ms: u64,
) -> Result<SceneMesh, RendererPlanError> {
    let puppet = reader.record_at(
        SceneBinaryChunkKind::Puppet,
        SCENE_BINARY_PUPPET_RECORD_SIZE,
        puppet_index,
        decode_puppet_record,
    )?;
    if puppet.animation_layer_count == 0 || puppet.bone_count == 0 || puppet.clip_count == 0 {
        return Ok(mesh);
    }
    mesh.skin = Some(binary_scene_puppet_skin(reader, puppet)?);
    mesh.puppet_clips = binary_scene_puppet_clips(reader, puppet)?;
    let layers = binary_scene_puppet_layers(reader, puppet)?;
    Ok(mesh
        .sample_puppet_animation(&layers, snapshot_time_ms)
        .unwrap_or(mesh))
}

fn binary_scene_puppet_skin(
    reader: &mut BinarySceneReader,
    puppet: crate::core::scene::binary::SceneBinaryPuppetRecord,
) -> Result<SceneMeshSkin, RendererPlanError> {
    let bone_records = reader.record_range(
        SceneBinaryChunkKind::PuppetSkinBones,
        SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
        puppet.first_bone,
        puppet.bone_count,
        decode_puppet_skin_bone_record,
    )?;
    let vertex_records = reader.record_range(
        SceneBinaryChunkKind::PuppetSkinVertices,
        SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
        puppet.first_skin_vertex,
        puppet.skin_vertex_count,
        decode_puppet_skin_vertex_record,
    )?;
    let mut bones = Vec::with_capacity(bone_records.len());
    for bone in bone_records {
        bones.push(SceneMeshSkinBone {
            parent: (bone.parent_index != SCENE_BINARY_NONE_ID)
                .then_some(bone.parent_index as usize),
            bind: bone.transform,
        });
    }
    let mut vertices = Vec::with_capacity(vertex_records.len());
    for vertex in vertex_records {
        vertices.push(SceneMeshSkinVertex {
            bone_indices: [
                vertex.bone_indices[0] as usize,
                vertex.bone_indices[1] as usize,
                vertex.bone_indices[2] as usize,
                vertex.bone_indices[3] as usize,
            ],
            weights: [
                f64::from(vertex.weights[0]),
                f64::from(vertex.weights[1]),
                f64::from(vertex.weights[2]),
                f64::from(vertex.weights[3]),
            ],
        });
    }
    Ok(SceneMeshSkin {
        bones,
        vertices,
        attachments: Vec::new(),
    })
}

fn binary_scene_puppet_clips(
    reader: &mut BinarySceneReader,
    puppet: crate::core::scene::binary::SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationClip>, RendererPlanError> {
    let clip_records = reader.record_range(
        SceneBinaryChunkKind::PuppetClips,
        SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
        puppet.first_clip,
        puppet.clip_count,
        decode_puppet_clip_record,
    )?;
    let mut clips = Vec::with_capacity(clip_records.len());
    for clip in clip_records {
        let frame_records = reader.record_range(
            SceneBinaryChunkKind::PuppetFrames,
            SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            clip.first_frame,
            clip.frame_record_count,
            decode_puppet_frame_record,
        )?;
        let mut bones = (0..clip.bone_count)
            .map(|_| ScenePuppetAnimationBone { frames: Vec::new() })
            .collect::<Vec<_>>();
        for frame in frame_records {
            if let Some(bone) = bones.get_mut(frame.bone_index as usize) {
                bone.frames.push(frame.transform);
            }
        }
        clips.push(ScenePuppetAnimationClip {
            id: clip.clip_id,
            name: None,
            fps: f64::from(clip.fps),
            frame_count: clip.frame_count,
            looping: clip.flags & crate::core::scene::binary::SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING
                != 0,
            bones,
        });
    }
    Ok(clips)
}

fn binary_scene_puppet_layers(
    reader: &mut BinarySceneReader,
    puppet: crate::core::scene::binary::SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationLayer>, RendererPlanError> {
    let layer_records = reader.record_range(
        SceneBinaryChunkKind::PuppetLayers,
        SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
        puppet.first_layer,
        puppet.animation_layer_count,
        decode_puppet_layer_record,
    )?;
    let mut layers = Vec::with_capacity(layer_records.len());
    for layer in layer_records {
        layers.push(ScenePuppetAnimationLayer {
            clip_id: layer.clip_id,
            name: None,
            additive: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE != 0,
            lock_transforms: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS != 0,
            blend: f64::from(layer.blend),
            visible: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE != 0,
            rate: f64::from(layer.rate),
            initial_phase: f64::from(layer.initial_phase),
        });
    }
    Ok(layers)
}

#[derive(Debug, Clone, Copy)]
struct BinarySceneNodeState {
    transform: SceneTransform,
    opacity: f64,
    width: Option<f64>,
    height: Option<f64>,
    corner_radius: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct BinarySceneEffectiveNodeState {
    visible: bool,
    state: BinarySceneNodeState,
}

fn binary_scene_node_state(
    reader: &mut BinarySceneReader,
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    geometry: Option<SceneBinaryGeometryRecord>,
    snapshot_time_ms: u64,
) -> Result<BinarySceneNodeState, RendererPlanError> {
    let mut state = BinarySceneNodeState {
        transform: SceneTransform::default(),
        opacity: f64::from(node.opacity),
        width: geometry
            .and_then(|geometry| (geometry.width > 0.0).then_some(f64::from(geometry.width))),
        height: geometry
            .and_then(|geometry| (geometry.height > 0.0).then_some(f64::from(geometry.height))),
        corner_radius: (node.flags & BINARY_NODE_FLAG_CORNER_RADIUS != 0)
            .then_some(f64::from(node.corner_radius)),
    };
    let records = reader.record_range(
        SceneBinaryChunkKind::TransformTimeline,
        SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
        node.first_transform,
        node.transform_count,
        decode_transform_timeline_record,
    )?;
    for record in records {
        if record.property == BINARY_TRANSFORM_PROPERTY_DEFAULT {
            state.transform = binary_scene_default_transform(record);
            continue;
        }
        if record.keyframe_count == 0 {
            continue;
        }
        let Some(value) = binary_scene_transform_timeline_value(reader, record, snapshot_time_ms)?
        else {
            continue;
        };
        binary_scene_apply_timeline_value(&mut state, record.property, value);
    }
    Ok(state)
}

fn binary_scene_parent_node_state(
    states: &[BinarySceneEffectiveNodeState],
    parent_index: u32,
) -> Result<Option<BinarySceneEffectiveNodeState>, RendererPlanError> {
    if parent_index == SCENE_BINARY_NONE_ID {
        return Ok(None);
    }
    let Some(state) = states.get(parent_index as usize).copied() else {
        return Err(RendererPlanError::PackageLoad(format!(
            "binary scene node parent index {parent_index} is not before its child"
        )));
    };
    Ok(Some(state))
}

fn binary_scene_effective_node_state(
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    local: BinarySceneNodeState,
    parent: Option<BinarySceneEffectiveNodeState>,
) -> BinarySceneEffectiveNodeState {
    let visible =
        node.flags & BINARY_NODE_FLAG_VISIBLE != 0 && parent.is_none_or(|parent| parent.visible);
    let Some(parent) = parent else {
        return BinarySceneEffectiveNodeState {
            visible,
            state: local,
        };
    };
    BinarySceneEffectiveNodeState {
        visible,
        state: BinarySceneNodeState {
            transform: binary_scene_compose_transform(parent.state.transform, local.transform),
            opacity: (parent.state.opacity * local.opacity).clamp(0.0, 1.0),
            width: local.width,
            height: local.height,
            corner_radius: local.corner_radius,
        },
    }
}

fn binary_scene_compose_transform(parent: SceneTransform, child: SceneTransform) -> SceneTransform {
    let rotation = parent.rotation_deg.to_radians();
    let child_x = child.x * parent.scale_x;
    let child_y = child.y * parent.scale_y;
    let rotated_child_x = child_x.mul_add(rotation.cos(), -child_y * rotation.sin());
    let rotated_child_y = child_x.mul_add(rotation.sin(), child_y * rotation.cos());
    SceneTransform {
        x: parent.x + rotated_child_x,
        y: parent.y + rotated_child_y,
        scale_x: parent.scale_x * child.scale_x,
        scale_y: parent.scale_y * child.scale_y,
        rotation_deg: parent.rotation_deg + child.rotation_deg,
        anchor_x: child.anchor_x,
        anchor_y: child.anchor_y,
    }
}

fn binary_scene_default_transform(
    record: crate::core::scene::binary::SceneBinaryTransformTimelineRecord,
) -> SceneTransform {
    SceneTransform {
        x: f64::from(record.value0),
        y: f64::from(record.value1),
        scale_x: f64::from(record.value2),
        scale_y: f64::from(record.value3),
        rotation_deg: f64::from(record.value4),
        anchor_x: f64::from(record.value5),
        anchor_y: f64::from(record.value6),
    }
}

fn binary_scene_transform_timeline_value(
    reader: &mut BinarySceneReader,
    record: crate::core::scene::binary::SceneBinaryTransformTimelineRecord,
    snapshot_time_ms: u64,
) -> Result<Option<f64>, RendererPlanError> {
    let keyframes = reader.record_range(
        SceneBinaryChunkKind::TransformKeyframes,
        SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
        record.first_keyframe,
        record.keyframe_count,
        decode_transform_keyframe_record,
    )?;
    let mut keyframes = keyframes.into_iter();
    let Some(first) = keyframes.next() else {
        return Ok(None);
    };
    if record.keyframe_count == 1 {
        return Ok(Some(f64::from(first.value)));
    }
    let mut time_ms = snapshot_time_ms.saturating_add(record.time_offset_ms);
    if record.flags & BINARY_TRANSFORM_FLAG_LOOP != 0 && record.last_time_ms > 0 {
        time_ms %= record.last_time_ms;
    }
    if time_ms <= first.time_ms {
        return Ok(Some(f64::from(first.value)));
    }
    let mut previous = first;
    for next in keyframes {
        if time_ms <= next.time_ms {
            let span = next.time_ms.saturating_sub(previous.time_ms) as f64;
            let progress = if span > 0.0 {
                (time_ms.saturating_sub(previous.time_ms) as f64 / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let eased = binary_scene_curve_ease(next.curve, progress);
            return Ok(Some(
                f64::from(previous.value)
                    + (f64::from(next.value) - f64::from(previous.value)) * eased,
            ));
        }
        previous = next;
    }
    Ok(Some(f64::from(previous.value)))
}

fn binary_scene_apply_timeline_value(state: &mut BinarySceneNodeState, property: u16, value: f64) {
    match property {
        BINARY_TRANSFORM_PROPERTY_X => state.transform.x = value,
        BINARY_TRANSFORM_PROPERTY_Y => state.transform.y = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_X if value > 0.0 => state.transform.scale_x = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_Y if value > 0.0 => state.transform.scale_y = value,
        BINARY_TRANSFORM_PROPERTY_OPACITY => state.opacity = value.clamp(0.0, 1.0),
        BINARY_TRANSFORM_PROPERTY_ROTATION_DEG => state.transform.rotation_deg = value,
        BINARY_TRANSFORM_PROPERTY_WIDTH => state.width = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_HEIGHT => state.height = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS => state.corner_radius = Some(value.max(0.0)),
        _ => {}
    }
}

fn binary_scene_curve_ease(code: u16, value: f64) -> f64 {
    match code {
        2 => {
            if value >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        3 => value * value,
        4 => 1.0 - (1.0 - value) * (1.0 - value),
        5 => {
            if value < 0.5 {
                2.0 * value * value
            } else {
                1.0 - (-2.0 * value + 2.0).powi(2) / 2.0
            }
        }
        _ => value,
    }
}

fn binary_scene_effect_uv_transform(
    record: SceneBinaryEffectUvTransformRecord,
) -> SceneEffectUvTransform {
    SceneEffectUvTransform {
        mapping: match record.mapping {
            SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION => {
                SceneEffectUvMapping::TextureResolution
            }
            _ => SceneEffectUvMapping::TextureResolution,
        },
        source_slot: record.source_slot,
        mask_slot: record.mask_slot,
        scale: [f64::from(record.scale_u), f64::from(record.scale_v)],
        offset: [f64::from(record.offset_u), f64::from(record.offset_v)],
        input_extent: (record.flags & BINARY_EFFECT_UV_HAS_INPUT_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.input_width, record.input_height))
            .flatten(),
        mask_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.mask_width, record.mask_height))
            .flatten(),
        mask_backing_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.backing_width, record.backing_height))
            .flatten(),
    }
}

fn binary_scene_effect_uv_extent(width: u32, height: u32) -> Option<SceneEffectUvExtent> {
    (width > 0 && height > 0).then_some(SceneEffectUvExtent { width, height })
}

fn binary_resource_by_name(
    resources: &[BinarySceneResource],
    id_name: u32,
) -> Option<&BinarySceneResource> {
    (id_name != SCENE_BINARY_NONE_ID)
        .then(|| {
            resources
                .iter()
                .find(|resource| resource.id_name == id_name)
        })
        .flatten()
}

fn binary_scene_alpha_texture_slot(material: SceneBinaryMaterialPassRecord) -> Option<u32> {
    (material.alpha_texture_slot != SCENE_BINARY_NONE_ID).then_some(material.alpha_texture_slot)
}

fn binary_scene_alpha_texture_mode(
    material: SceneBinaryMaterialPassRecord,
) -> SceneRenderAlphaTextureMode {
    match material.alpha_texture_mode {
        2 => SceneRenderAlphaTextureMode::Inverse,
        3 => SceneRenderAlphaTextureMode::Iris,
        4 => SceneRenderAlphaTextureMode::Coverage,
        _ => SceneRenderAlphaTextureMode::Multiply,
    }
}

fn binary_scene_blend_mode(code: u16) -> SceneBlendMode {
    match code {
        2 => SceneBlendMode::Additive,
        3 => SceneBlendMode::Multiply,
        4 => SceneBlendMode::Screen,
        5 => SceneBlendMode::Max,
        6 => SceneBlendMode::Normal,
        _ => SceneBlendMode::Alpha,
    }
}

fn binary_scene_fit(code: u16) -> FitMode {
    match code {
        2 => FitMode::Contain,
        3 => FitMode::Stretch,
        4 => FitMode::Tile,
        5 => FitMode::Center,
        _ => FitMode::Cover,
    }
}

fn binary_scene_text_align(code: u16) -> Option<SceneTextAlign> {
    match code {
        2 => Some(SceneTextAlign::Middle),
        3 => Some(SceneTextAlign::End),
        1 => Some(SceneTextAlign::Start),
        _ => None,
    }
}

fn binary_scene_node_kind(code: u16) -> Option<SceneNodeKind> {
    Some(match code {
        1 => SceneNodeKind::Image,
        2 => SceneNodeKind::Video,
        3 => SceneNodeKind::Color,
        4 => SceneNodeKind::Rectangle,
        5 => SceneNodeKind::Ellipse,
        6 => SceneNodeKind::Text,
        7 => SceneNodeKind::Path,
        10 => SceneNodeKind::ParticleEmitter,
        11 => SceneNodeKind::AudioResponse,
        _ => return None,
    })
}

fn binary_scene_node_kind_is_renderable(kind: SceneNodeKind) -> bool {
    matches!(
        kind,
        SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::ParticleEmitter
            | SceneNodeKind::AudioResponse
    )
}

fn binary_scene_effect_runtime(kind: u16, effect_file: &str) -> Option<String> {
    let normalized = effect_file.replace('\\', "/").to_ascii_lowercase();
    let runtime = match kind {
        1 => "native-opacity-mask",
        2 => "native-iris-mask",
        3..=5 | 7..=9 => "native-effect-motion",
        6 => "native-water-caustics",
        _ if normalized.ends_with("effects/opacity/effect.json") => "native-opacity-mask",
        _ if normalized.ends_with("effects/iris/effect.json") => "native-iris-mask",
        _ if normalized.contains("waterripple")
            || normalized.contains("waterwaves")
            || normalized.contains("waterflow")
            || normalized.contains("sway")
            || normalized.contains("shake")
            || normalized.contains("flutter")
            || normalized.contains("drift") =>
        {
            "native-effect-motion"
        }
        _ => return None,
    };
    Some(runtime.to_owned())
}

fn binary_scene_material_flag(code: u16) -> Option<String> {
    match code {
        1 => Some("enabled".to_owned()),
        2 => Some("disabled".to_owned()),
        _ => None,
    }
}

fn binary_scene_cull_mode(code: u16) -> Option<String> {
    match code {
        1 => Some("disabled".to_owned()),
        2 => Some("back".to_owned()),
        3 => Some("front".to_owned()),
        4 => Some("frontandback".to_owned()),
        5 => Some("unknown".to_owned()),
        _ => None,
    }
}

fn binary_scene_flagged_color(flags: u16, flag: u16, rgba: u32) -> Option<String> {
    (flags & flag != 0).then(|| binary_scene_rgba_hex(rgba))
}

fn binary_scene_rgba_hex(rgba: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        rgba >> 24,
        (rgba >> 16) & 0xff,
        (rgba >> 8) & 0xff
    )
}

fn binary_name(names: &BinarySceneNames, id: u32) -> Option<&str> {
    names.name(id)
}

fn binary_scene_resource_path(package_root: &Path, source: &str) -> PathBuf {
    let source = PathBuf::from(source);
    if source.is_absolute() {
        source
    } else {
        package_root.join(source)
    }
}

fn binary_scene_package_root(source_path: &Path) -> PathBuf {
    let Some(parent) = source_path.parent() else {
        return PathBuf::from(".");
    };
    if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "assets")
        && let Some(root) = parent.parent()
    {
        return root.to_path_buf();
    }
    parent.to_path_buf()
}

fn binary_plan_error(err: SceneBinaryError) -> RendererPlanError {
    RendererPlanError::PackageLoad(format!("failed to read binary scene: {err}"))
}

fn binary_scene_read_exact_at(
    file: &mut File,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, RendererPlanError> {
    file.seek(SeekFrom::Start(offset)).map_err(|err| {
        binary_plan_error(SceneBinaryError::StreamIo {
            operation: "seek",
            message: err.to_string(),
        })
    })?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).map_err(|err| {
        binary_plan_error(SceneBinaryError::StreamIo {
            operation: "read",
            message: err.to_string(),
        })
    })?;
    Ok(bytes)
}

fn binary_scene_read_u32(bytes: &[u8], offset: usize) -> Result<u32, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 4,
            actual: bytes.len(),
        })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn binary_scene_read_u64(bytes: &[u8], offset: usize) -> Result<u64, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 8,
            actual: bytes.len(),
        })?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::encode_scene_binary_document;

    #[test]
    fn gscn_direct_ingest_expands_particle_emitters_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "spark", "type": "image", "source": "assets/spark.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "parent",
                    "type": "group",
                    "opacity": 0.5,
                    "transform": { "x": 100.0, "y": 50.0 },
                    "children": [
                        {
                            "id": "spark-emitter",
                            "type": "particle-emitter",
                            "resource": "spark",
                            "opacity": 0.8,
                            "transform": { "x": 10.0, "y": 20.0 },
                            "properties": {
                                "particle": {
                                    "count": 3,
                                    "seed": 1,
                                    "lifetime_ms": 1000,
                                    "loop": true,
                                    "spawn_width": 0.0,
                                    "spawn_height": 0.0,
                                    "width": 6.0,
                                    "height": 8.0,
                                    "speed": 0.0,
                                    "spread_deg": 0.0,
                                    "gravity_x": 0.0,
                                    "gravity_y": 0.0,
                                    "fade": false,
                                    "color": "#aabbcc"
                                }
                            }
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-particle-plan");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan =
            scene_wallpaper_plan_from_gscn_path("HDMI-A-1".to_owned(), scene_path, None, 250, None)
                .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 3);
        for layer in &plan.layers {
            assert_eq!(layer.id, "");
            assert_eq!(layer.kind, SceneNodeKind::Image);
            assert_eq!(layer.texture_slots.len(), 1);
            assert_eq!(layer.color.as_deref(), Some("#aabbcc"));
            assert_eq!(layer.width, Some(6.0));
            assert_eq!(layer.height, Some(8.0));
            assert!((layer.opacity - 0.4).abs() < 1e-6);
            assert!((layer.transform.x - 110.0).abs() < f64::EPSILON);
            assert!((layer.transform.y - 70.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn gscn_binary_runtime_sampler_reads_timeline_frames_without_json() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "hero", "type": "image", "source": "assets/hero.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "hero-node",
                    "type": "image",
                    "resource": "hero",
                    "width": 16.0,
                    "height": 16.0,
                    "transform": { "x": 0.0, "y": 5.0 }
                }
            ],
            "timelines": [
                {
                    "id": "move-x",
                    "target_node": "hero-node",
                    "channels": [
                        {
                            "property": "x",
                            "keyframes": [
                                { "time_ms": 0, "value": 0.0 },
                                { "time_ms": 1000, "value": 100.0 }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-runtime-sampler");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path,
            Some(60),
            0,
            None,
        )
        .expect("binary scene plan");
        let mut sampler = SceneBinaryRuntimeSampler::from_plan(&plan)
            .expect("binary sampler open")
            .expect("binary sampler");
        let frame = sampler.sample_frame_reusing(500).expect("sample frame");

        assert_eq!(frame.snapshot_time_ms, 500);
        assert_eq!(frame.layers.len(), 1);
        assert!((frame.layers[0].transform.x - 50.0).abs() < 0.0001);
        assert!((frame.layers[0].transform.y - 5.0).abs() < 0.0001);

        sampler.recycle_frame(frame);
        fs::remove_dir_all(root).expect("remove test dir");
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
