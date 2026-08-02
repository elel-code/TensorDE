//! Wallpaper Engine MDLV mesh parser for cold-path scene IR ingest.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/mdl-format.md`
//! - `reverse-engineered/gilder/docs/exe/model-and-animation.md`
//! - `reverse-engineered/gilder/shaders/**`

use std::fmt;

use crate::engine::scene::abi::SceneVec3;

mod decoder;

use decoder::{MdlDecoder, f32_at, u32_at};

#[derive(Debug, Clone, PartialEq)]
pub struct MdlModel {
    pub version: u32,
    pub layout_mask: u32,
    pub material_paths: Vec<String>,
    pub entries: Vec<MdlMeshEntry>,
    pub bones: Vec<MdlBone>,
    pub attachments: Vec<MdlAttachment>,
    pub animations: Vec<MdlAnimationClip>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlMeshEntry {
    pub entry_flags: u32,
    pub entry_layout_mask: u32,
    pub bounds: [f32; 6],
    pub vertices: Vec<MdlMeshVertex>,
    pub indices: Vec<u32>,
    pub source_records: Vec<MdlSourceRecord>,
    pub clipping_subdraws: Vec<MdlClippingSubdraw>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MdlSourceRecord {
    pub source_index: u32,
    pub local_index_offset: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdlClippingSubdraw {
    pub source_qword: u64,
    pub mask_resource: String,
    pub raw_flags: u32,
    /// Source-record spans drawn with CLIPPINGTARGET after the mask pass.
    pub target_source_ordinals: Vec<u32>,
    /// Source-record spans drawn with clippingmaskimage4 into FullAlphaMask.
    pub mask_source_ordinals: Vec<u32>,
}

impl MdlMeshEntry {
    pub fn non_producer_indices(&self) -> Vec<u32> {
        if self.source_records.is_empty() || self.clipping_subdraws.is_empty() {
            return self.indices.clone();
        }
        let target_ordinals = self
            .clipping_subdraws
            .iter()
            .flat_map(|subdraw| subdraw.target_source_ordinals.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        let mut mask_only = vec![false; self.indices.len()];
        let index_count = mask_only.len();
        for ordinal in target_ordinals {
            let Some(record) = self.source_records.get(ordinal as usize) else {
                continue;
            };
            let start = record.index_start as usize;
            let end = start
                .saturating_add(record.index_count as usize)
                .min(index_count);
            mask_only[start.min(index_count)..end].fill(true);
        }
        self.indices
            .iter()
            .copied()
            .zip(mask_only)
            .filter_map(|(index, mask_only)| (!mask_only).then_some(index))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlMeshVertex {
    pub position: SceneVec3,
    pub uv: [f32; 2],
    pub blend_indices: [u32; 4],
    pub blend_weights: [f32; 4],
}

pub fn mdl_entry_vertex_bounds(entry: &MdlMeshEntry) -> (SceneVec3, SceneVec3) {
    let mut min = entry.vertices[0].position;
    let mut max = entry.vertices[0].position;
    for vertex in &entry.vertices[1..] {
        min.x = min.x.min(vertex.position.x);
        min.y = min.y.min(vertex.position.y);
        min.z = min.z.min(vertex.position.z);
        max.x = max.x.max(vertex.position.x);
        max.y = max.y.max(vertex.position.y);
        max.z = max.z.max(vertex.position.z);
    }
    (min, max)
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlAttachment {
    pub bone_index: u32,
    pub name: String,
    pub local_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlBone {
    pub bone_index: u32,
    pub name: String,
    pub simulation_type: i32,
    pub parent_index: i32,
    pub local_bind_matrix: [f32; 16],
    pub simulation_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlAnimationClip {
    pub clip_id: u32,
    pub flags: u32,
    pub name: String,
    pub playback: String,
    pub fps: f32,
    pub frame_count: u32,
    pub frame_metadata: u32,
    pub tracks: Vec<MdlAnimationTrack>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlAnimationTrack {
    pub bone_index: u32,
    pub track_flags: u32,
    pub samples: Vec<MdlAnimationTransformSample>,
    pub opacity_flags: u32,
    pub opacity_samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MdlAnimationTransformSample {
    pub translation: SceneVec3,
    pub rotation: SceneVec3,
    pub scale: SceneVec3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdlParseError {
    TooShort,
    InvalidMagic(String),
    InvalidVersion(String),
    UnexpectedEof(&'static str),
    InvalidUtf8String {
        field: &'static str,
        offset: usize,
    },
    UnsupportedVertexStride {
        layout_mask: u32,
        vertex_bytes: u32,
    },
    InvalidIndexBytes(u32),
    InvalidBoneMatrixBytes(u32),
    InvalidAnimationTrackBytes {
        clip_id: u32,
        bone_ordinal: u32,
        byte_count: u32,
    },
}

impl fmt::Display for MdlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => f.write_str("WE MDL file is too short"),
            Self::InvalidMagic(magic) => write!(f, "unsupported WE MDL magic {magic:?}"),
            Self::InvalidVersion(version) => write!(f, "invalid WE MDL version {version:?}"),
            Self::UnexpectedEof(field) => {
                write!(f, "unexpected end of WE MDL while reading {field}")
            }
            Self::InvalidUtf8String { field, offset } => {
                write!(f, "WE MDL {field} at byte {offset} is not valid UTF-8")
            }
            Self::UnsupportedVertexStride {
                layout_mask,
                vertex_bytes,
            } => write!(
                f,
                "unsupported WE MDL vertex layout 0x{layout_mask:08x} with {vertex_bytes} bytes"
            ),
            Self::InvalidIndexBytes(bytes) => {
                write!(f, "WE MDL index block has odd byte length {bytes}")
            }
            Self::InvalidBoneMatrixBytes(bytes) => {
                write!(f, "WE MDLS bone matrix has {bytes} bytes instead of 64")
            }
            Self::InvalidAnimationTrackBytes {
                clip_id,
                bone_ordinal,
                byte_count,
            } => write!(
                f,
                "WE MDLA clip {clip_id} bone track {bone_ordinal} has invalid byte length {byte_count}"
            ),
        }
    }
}

impl std::error::Error for MdlParseError {}

pub fn parse_mdl_model(bytes: &[u8]) -> Result<MdlModel, MdlParseError> {
    let mut decoder = MdlDecoder::new(bytes);
    let magic = decoder.bytes(9, "magic")?;
    if magic.len() != 9 {
        return Err(MdlParseError::TooShort);
    }
    if &magic[..4] != b"MDLV" || magic[8] != 0 {
        return Err(MdlParseError::InvalidMagic(
            String::from_utf8_lossy(magic).into_owned(),
        ));
    }
    let version_text = std::str::from_utf8(&magic[4..8])
        .map_err(|_| MdlParseError::InvalidVersion(String::from_utf8_lossy(&magic[4..8]).into()))?;
    let version = version_text
        .parse::<u32>()
        .map_err(|_| MdlParseError::InvalidVersion(version_text.to_owned()))?;
    let layout_mask = decoder.u32("layout_mask")?;
    let material_count = decoder.u32("material_count")?;
    let entry_count = decoder.u32("entry_count")?;

    let material_capacity = decoder.checked_count(material_count, 1, "material_count")?;
    let mut material_paths = Vec::with_capacity(material_capacity);
    for _ in 0..material_count {
        material_paths.push(decoder.c_string("material_path")?);
    }

    let entry_capacity = decoder.checked_count(entry_count, 1, "entry_count")?;
    let mut entries = Vec::with_capacity(entry_capacity);
    for _ in 0..entry_count {
        entries.push(parse_entry(&mut decoder, version, layout_mask)?);
    }
    let bones = parse_mdls_bones(bytes)?;
    let attachments = parse_mdat_attachments(bytes)?;
    let animations = parse_mdla_animations(bytes, &bones)?;

    Ok(MdlModel {
        version,
        layout_mask,
        material_paths,
        entries,
        bones,
        attachments,
        animations,
    })
}

fn parse_mdls_bones(bytes: &[u8]) -> Result<Vec<MdlBone>, MdlParseError> {
    let Some(marker_offset) = find_bytes(bytes, b"MDLS0004") else {
        return Ok(Vec::new());
    };
    let metadata_offset =
        mdl_section_metadata_offset(bytes, marker_offset, 78, "mdls_section_metadata")?;
    parse_mdls_bones_from(bytes, metadata_offset)
}

fn parse_mdls_bones_from(bytes: &[u8], offset: usize) -> Result<Vec<MdlBone>, MdlParseError> {
    let mut decoder = MdlDecoder { bytes, offset };
    let _section_end = decoder.u32("mdls_section_end")?;
    let bone_count = decoder.u32("mdls_bone_count")?;
    let bone_capacity = decoder.checked_count(bone_count, 78, "mdls_bone_records")?;
    let mut bones = Vec::with_capacity(bone_capacity);
    for bone_index in 0..bone_count {
        let name = decoder.c_string("mdls_bone_name")?;
        let simulation_type = decoder.i32("mdls_simulation_type")?;
        let parent_raw = decoder.u32("mdls_parent_index")?;
        let parent_index = if parent_raw == u32::MAX || parent_raw >= bone_index {
            -1
        } else {
            parent_raw as i32
        };
        let matrix_byte_length = decoder.u32("mdls_matrix_byte_length")? as usize;
        if matrix_byte_length != 64 {
            return Err(MdlParseError::InvalidBoneMatrixBytes(
                matrix_byte_length as u32,
            ));
        }
        let matrix_payload = decoder.bytes(matrix_byte_length, "mdls_matrix_payload")?;
        let mut local_bind_matrix = [0.0; 16];
        for (slot, chunk) in matrix_payload.chunks_exact(4).take(16).enumerate() {
            local_bind_matrix[slot] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let simulation_json = decoder.c_string("mdls_simulation_json")?;
        bones.push(MdlBone {
            bone_index,
            name,
            simulation_type,
            parent_index,
            local_bind_matrix,
            simulation_json,
        });
    }
    Ok(bones)
}

fn parse_mdat_attachments(bytes: &[u8]) -> Result<Vec<MdlAttachment>, MdlParseError> {
    let Some(marker_offset) = find_bytes(bytes, b"MDAT0001\0") else {
        return Ok(Vec::new());
    };
    let mut decoder = MdlDecoder {
        bytes,
        offset: marker_offset + 9,
    };
    let _section_end = decoder.u32("mdat_section_end")?;
    let attachment_count = decoder.u16("mdat_attachment_count")?;
    let attachment_capacity =
        decoder.checked_count(attachment_count.into(), 67, "mdat_attachment_records")?;
    let mut attachments = Vec::with_capacity(attachment_capacity);
    for _ in 0..attachment_count {
        let bone_index = decoder.u16("mdat_attachment_bone_index")? as u32;
        let name = decoder.c_string("mdat_attachment_name")?;
        let mut local_matrix = [0.0; 16];
        for item in &mut local_matrix {
            *item = decoder.f32("mdat_attachment_local_matrix")?;
        }
        attachments.push(MdlAttachment {
            bone_index,
            name,
            local_matrix,
        });
    }
    Ok(attachments)
}

fn parse_mdla_animations(
    bytes: &[u8],
    bones: &[MdlBone],
) -> Result<Vec<MdlAnimationClip>, MdlParseError> {
    let Some(marker_offset) = find_bytes(bytes, b"MDLA0006") else {
        return Ok(Vec::new());
    };
    let metadata_offset =
        mdl_section_metadata_offset(bytes, marker_offset, 26, "mdla_section_metadata")?;
    parse_mdla_animations_from(bytes, metadata_offset, bones)
}

fn parse_mdla_animations_from(
    bytes: &[u8],
    offset: usize,
    bones: &[MdlBone],
) -> Result<Vec<MdlAnimationClip>, MdlParseError> {
    let mut decoder = MdlDecoder { bytes, offset };
    let section_end = mdl_section_end(
        bytes,
        decoder.u32("mdla_section_end")?,
        decoder.offset,
        "mdla_section_end",
    )?;
    let clip_count = decoder.u32("mdla_clip_count")?;
    let clip_capacity = decoder.checked_count(clip_count, 26, "mdla_clip_records")?;
    let expected_bone_count = bones.len().min(u32::MAX as usize) as u32;
    let mut clips = Vec::with_capacity(clip_capacity);
    for clip_index in 0..clip_count {
        if clip_index != 0 {
            decoder.skip_zero_padding(section_end);
            if !plausible_mdla_clip_header(bytes, decoder.offset, section_end, expected_bone_count)
                && let Some(next_header) = find_next_mdla_clip_header(
                    bytes,
                    decoder.offset,
                    section_end,
                    expected_bone_count,
                )
            {
                decoder.offset = next_header;
            }
        }
        let clip_id = decoder.u32("mdla_clip_id")?;
        let flags = decoder.u32("mdla_flags")?;
        let name = decoder.c_string("mdla_name")?;
        let playback = decoder.c_string("mdla_playback")?;
        let fps = decoder.f32("mdla_fps")?;
        let frame_count = decoder.u32("mdla_frame_count")?;
        let frame_metadata = decoder.u32("mdla_frame_metadata")?;
        let bone_count = decoder.u32("mdla_bone_count")?;
        let track_capacity = decoder.checked_count(bone_count, 8, "mdla_track_records")?;
        let mut tracks = Vec::with_capacity(track_capacity);
        for bone_ordinal in 0..bone_count {
            let track_flags = decoder.u32("mdla_track_flags")?;
            let byte_count = decoder.u32("mdla_track_byte_count")?;
            if byte_count % 36 != 0 {
                return Err(MdlParseError::InvalidAnimationTrackBytes {
                    clip_id,
                    bone_ordinal,
                    byte_count,
                });
            }
            let sample_count = byte_count / 36;
            let sample_capacity =
                decoder.checked_count(sample_count, 36, "mdla_transform_samples")?;
            let mut samples = Vec::with_capacity(sample_capacity);
            for _ in 0..sample_count {
                samples.push(MdlAnimationTransformSample {
                    translation: decoder.vec3("mdla_translation")?,
                    rotation: decoder.vec3("mdla_rotation")?,
                    scale: decoder.vec3("mdla_scale")?,
                });
            }
            tracks.push(MdlAnimationTrack {
                bone_index: bones
                    .get(bone_ordinal as usize)
                    .map(|bone| bone.bone_index)
                    .unwrap_or(bone_ordinal),
                track_flags,
                samples,
                opacity_flags: 0,
                opacity_samples: Vec::new(),
            });
        }
        let remaining_clip_count = clip_count.saturating_sub(clip_index + 1);
        let next_clip_already_follows = remaining_clip_count != 0
            && plausible_mdla_clip_header(bytes, decoder.offset, section_end, bone_count);
        if !next_clip_already_follows
            && let Some((end, opacity_tracks)) = parse_mdla_opacity_tracks(
                bytes,
                decoder.offset,
                section_end,
                bone_count,
                frame_count.saturating_add(1),
                remaining_clip_count,
            )
        {
            decoder.offset = end;
            for (track, (opacity_flags, opacity_samples)) in tracks.iter_mut().zip(opacity_tracks) {
                track.opacity_flags = opacity_flags;
                track.opacity_samples = opacity_samples;
            }
        }
        clips.push(MdlAnimationClip {
            clip_id,
            flags,
            name,
            playback,
            fps,
            frame_count,
            frame_metadata,
            tracks,
        });
    }
    Ok(clips)
}

type MdlaOpacityTracks = (usize, Vec<(u32, Vec<f32>)>);

fn parse_mdla_opacity_tracks(
    bytes: &[u8],
    offset: usize,
    section_end: usize,
    bone_count: u32,
    sample_count: u32,
    remaining_clip_count: u32,
) -> Option<MdlaOpacityTracks> {
    let track_bytes = usize::try_from(sample_count).ok()?.checked_mul(4)?;
    let block_bytes = track_bytes.checked_add(8)?;
    let total_bytes = usize::try_from(bone_count).ok()?.checked_mul(block_bytes)?;
    for preamble_bytes in 0..=16usize {
        let base = offset.checked_add(preamble_bytes)?;
        let end = base.checked_add(total_bytes)?;
        if end > section_end || end > bytes.len() {
            continue;
        }
        let mut tracks = Vec::with_capacity(bone_count as usize);
        let mut valid = true;
        for bone_ordinal in 0..bone_count as usize {
            let block = base + bone_ordinal * block_bytes;
            let flags = u32_at(bytes, block);
            let byte_count = u32_at(bytes, block + 4) as usize;
            if byte_count != track_bytes {
                valid = false;
                break;
            }
            let samples = bytes[block + 8..block + 8 + track_bytes]
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("opacity f32")))
                .collect::<Vec<_>>();
            if samples.iter().any(|sample| !sample.is_finite()) {
                valid = false;
                break;
            }
            tracks.push((flags, samples));
        }
        let lands_on_next_clip = remaining_clip_count == 0
            || plausible_mdla_clip_header(bytes, end, section_end, bone_count);
        if valid && lands_on_next_clip {
            return Some((end, tracks));
        }
    }
    None
}

fn plausible_mdla_clip_header(
    bytes: &[u8],
    mut offset: usize,
    section_end: usize,
    expected_bone_count: u32,
) -> bool {
    let section_end = section_end.min(bytes.len());
    while offset < section_end && bytes[offset] == 0 {
        offset += 1;
    }
    let Some(header) = bytes.get(offset..section_end) else {
        return false;
    };
    if header.len() < 8 || u32_at(header, 0) == 0 {
        return false;
    }
    let mut cursor = 8usize;
    for string_index in 0..2 {
        let Some(length) = header[cursor..].iter().position(|byte| *byte == 0) else {
            return false;
        };
        let value = &header[cursor..cursor + length];
        if std::str::from_utf8(value).is_err()
            || (string_index == 1
                && (value.is_empty()
                    || !value.iter().all(|byte| {
                        byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_'
                    })))
        {
            return false;
        }
        cursor += length + 1;
    }
    let Some(scalars) = header.get(cursor..cursor + 16) else {
        return false;
    };
    let fps = f32::from_le_bytes(scalars[..4].try_into().expect("fps slice"));
    let track_count = u32_at(scalars, 12);
    fps.is_finite() && fps > 0.0 && fps <= 1_000.0 && track_count == expected_bone_count
}

fn find_next_mdla_clip_header(
    bytes: &[u8],
    offset: usize,
    section_end: usize,
    expected_bone_count: u32,
) -> Option<usize> {
    const MAX_INTER_CLIP_METADATA_BYTES: usize = 64 * 1024;
    let scan_end = offset
        .saturating_add(MAX_INTER_CLIP_METADATA_BYTES)
        .min(section_end)
        .min(bytes.len());
    (offset..scan_end).find_map(|candidate| {
        plausible_mdla_clip_header(bytes, candidate, section_end, expected_bone_count).then(|| {
            bytes[candidate..scan_end]
                .iter()
                .position(|byte| *byte != 0)
                .map_or(candidate, |padding| candidate + padding)
        })
    })
}

fn mdl_section_end(
    bytes: &[u8],
    encoded_end: u32,
    payload_offset: usize,
    field: &'static str,
) -> Result<usize, MdlParseError> {
    if encoded_end == 0 {
        return Ok(bytes.len());
    }
    let end = encoded_end as usize;
    if end < payload_offset || end > bytes.len() {
        return Err(MdlParseError::UnexpectedEof(field));
    }
    Ok(end)
}

fn mdl_section_metadata_offset(
    bytes: &[u8],
    marker_offset: usize,
    minimum_item_bytes: usize,
    field: &'static str,
) -> Result<usize, MdlParseError> {
    for metadata_offset in [
        marker_offset.saturating_add(9),
        marker_offset.saturating_add(8),
    ] {
        let Some(header) = bytes.get(metadata_offset..metadata_offset.saturating_add(8)) else {
            continue;
        };
        let encoded_end = u32::from_le_bytes(header[..4].try_into().expect("section end"));
        let count = u32::from_le_bytes(header[4..].try_into().expect("section count")) as usize;
        let payload_offset = metadata_offset + 8;
        let section_end = if encoded_end == 0 {
            bytes.len()
        } else {
            encoded_end as usize
        };
        if section_end < payload_offset || section_end > bytes.len() {
            continue;
        }
        let Some(required) = count.checked_mul(minimum_item_bytes) else {
            continue;
        };
        if required <= section_end - payload_offset {
            return Ok(metadata_offset);
        }
    }
    Err(MdlParseError::UnexpectedEof(field))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_entry(
    decoder: &mut MdlDecoder<'_>,
    version: u32,
    file_layout_mask: u32,
) -> Result<MdlMeshEntry, MdlParseError> {
    let mut entry_flags = 0;
    if version >= 4 {
        entry_flags = decoder.u32("entry_flags")?;
        if entry_flags & 0x2 != 0 {
            let _ = decoder.u32("entry_extra")?;
        }
    }
    let mut bounds = [0.0; 6];
    if version >= 17 {
        for item in &mut bounds {
            *item = decoder.f32("entry_bounds")?;
        }
    }
    let entry_layout_mask = if version >= 15 {
        decoder.u32("entry_layout_mask")?
    } else {
        file_layout_mask
    };
    let vertex_bytes = decoder.u32("vertex_bytes")?;
    let vertex_payload = decoder.bytes(vertex_bytes as usize, "vertices")?;
    let vertices = parse_vertices(entry_layout_mask, vertex_bytes, vertex_payload)?;
    let index_bytes = decoder.u32("index_bytes")?;
    let index_payload = decoder.bytes(index_bytes as usize, "indices")?;
    let indices = parse_indices(index_bytes, index_payload)?;
    let (source_records, clipping_subdraws) = if version >= 21
        && decoder
            .bytes
            .get(decoder.offset)
            .is_some_and(|present| *present <= 1)
    {
        parse_v21_source_and_clipping_blocks(decoder, version)?
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(MdlMeshEntry {
        entry_flags,
        entry_layout_mask,
        bounds,
        vertices,
        indices,
        source_records,
        clipping_subdraws,
    })
}

fn parse_v21_source_and_clipping_blocks(
    decoder: &mut MdlDecoder<'_>,
    version: u32,
) -> Result<(Vec<MdlSourceRecord>, Vec<MdlClippingSubdraw>), MdlParseError> {
    let optional_a_present = decoder.u8("v21_optional_a_present")? != 0;
    if optional_a_present {
        let _value = decoder.u32("v21_optional_a_value")?;
        let byte_count = decoder.u32("v21_optional_a_bytes")? as usize;
        let _payload = decoder.bytes(byte_count, "v21_optional_a_payload")?;
    }
    let optional_b_present = decoder.u8("v21_optional_b_present")? != 0;
    let source_records = if optional_b_present {
        let byte_count = decoder.u32("v21_optional_b_bytes")? as usize;
        let payload = decoder.bytes(byte_count, "v21_optional_b_payload")?;
        payload
            .chunks_exact(16)
            .map(|record| MdlSourceRecord {
                source_index: u32_at(record, 0),
                local_index_offset: u32_at(record, 4),
                index_start: u32_at(record, 8),
                index_count: u32_at(record, 12),
            })
            .collect()
    } else {
        Vec::new()
    };
    if version < 23 {
        return Ok((source_records, Vec::new()));
    }
    let subdraw_count = decoder.u32("v23_subdraw_count")?;
    let mut clipping_subdraws = Vec::with_capacity(subdraw_count as usize);
    for _ in 0..subdraw_count {
        let source_qword = decoder.u64("v23_subdraw_source_qword")?;
        let mask_resource = decoder.c_string("v23_subdraw_mask_resource")?;
        let raw_flags = decoder.u32("v23_subdraw_flags")?;
        let target_count = decoder.u32("v23_subdraw_target_count")?;
        let mut target_source_ordinals = Vec::with_capacity(target_count as usize);
        for _ in 0..target_count {
            target_source_ordinals.push(decoder.u32("v23_subdraw_target_ordinal")?);
        }
        let mask_count = decoder.u32("v23_subdraw_mask_count")?;
        let mut mask_source_ordinals = Vec::with_capacity(mask_count as usize);
        for _ in 0..mask_count {
            mask_source_ordinals.push(decoder.u32("v23_subdraw_mask_ordinal")?);
        }
        clipping_subdraws.push(MdlClippingSubdraw {
            source_qword,
            mask_resource,
            raw_flags,
            target_source_ordinals,
            mask_source_ordinals,
        });
    }
    Ok((source_records, clipping_subdraws))
}

fn parse_vertices(
    layout_mask: u32,
    vertex_bytes: u32,
    payload: &[u8],
) -> Result<Vec<MdlMeshVertex>, MdlParseError> {
    const WE_MDLV0023_SKINNED_VERTEX_STRIDE: usize = 80;
    if !payload
        .len()
        .is_multiple_of(WE_MDLV0023_SKINNED_VERTEX_STRIDE)
    {
        return Err(MdlParseError::UnsupportedVertexStride {
            layout_mask,
            vertex_bytes,
        });
    }
    let mut vertices = Vec::with_capacity(payload.len() / WE_MDLV0023_SKINNED_VERTEX_STRIDE);
    for chunk in payload.chunks_exact(WE_MDLV0023_SKINNED_VERTEX_STRIDE) {
        vertices.push(MdlMeshVertex {
            position: SceneVec3 {
                x: f32_at(chunk, 0),
                y: f32_at(chunk, 4),
                z: f32_at(chunk, 8),
            },
            // WE MDL vertices and uploaded texture rows both use v=0 at the image top.
            uv: [f32_at(chunk, 72), f32_at(chunk, 76)],
            blend_indices: [
                u32_at(chunk, 40),
                u32_at(chunk, 44),
                u32_at(chunk, 48),
                u32_at(chunk, 52),
            ],
            blend_weights: [
                f32_at(chunk, 56),
                f32_at(chunk, 60),
                f32_at(chunk, 64),
                f32_at(chunk, 68),
            ],
        });
    }
    Ok(vertices)
}

fn parse_indices(index_bytes: u32, payload: &[u8]) -> Result<Vec<u32>, MdlParseError> {
    if !payload.len().is_multiple_of(2) {
        return Err(MdlParseError::InvalidIndexBytes(index_bytes));
    }
    Ok(payload
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]) as u32)
        .collect())
}

#[cfg(test)]
mod tests;
