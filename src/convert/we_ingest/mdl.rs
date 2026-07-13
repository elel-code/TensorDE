//! Wallpaper Engine MDLV mesh parser for cold-path scene IR ingest.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `reverse-engineered/shaders/**`

use std::fmt;

use crate::engine::scene::abi::SceneVec3;

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
    InvalidUtf8Path,
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
            Self::InvalidUtf8Path => f.write_str("WE MDL material path is not valid UTF-8"),
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
    let mut clips = Vec::with_capacity(clip_capacity);
    for clip_index in 0..clip_count {
        if clip_index != 0 {
            decoder.skip_zero_padding(section_end);
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
        if let Some((end, opacity_tracks)) = parse_mdla_opacity_tracks(
            bytes,
            decoder.offset,
            section_end,
            bone_count,
            frame_count.saturating_add(1),
        ) {
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

fn parse_mdla_opacity_tracks(
    bytes: &[u8],
    offset: usize,
    section_end: usize,
    bone_count: u32,
    sample_count: u32,
) -> Option<(usize, Vec<(u32, Vec<f32>)>)> {
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
        if valid {
            return Some((end, tracks));
        }
    }
    None
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
    Ok(MdlMeshEntry {
        entry_flags,
        entry_layout_mask,
        bounds,
        vertices,
        indices,
    })
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

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

struct MdlDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> MdlDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn bytes(&mut self, len: usize, field: &'static str) -> Result<&'a [u8], MdlParseError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        if end > self.bytes.len() {
            return Err(MdlParseError::UnexpectedEof(field));
        }
        let out = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(out)
    }

    fn checked_count(
        &self,
        count: u32,
        minimum_item_bytes: usize,
        field: &'static str,
    ) -> Result<usize, MdlParseError> {
        let count = count as usize;
        let required = count
            .checked_mul(minimum_item_bytes)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        if required > self.bytes.len().saturating_sub(self.offset) {
            return Err(MdlParseError::UnexpectedEof(field));
        }
        Ok(count)
    }

    fn skip_zero_padding(&mut self, limit: usize) {
        let limit = limit.min(self.bytes.len());
        while self.offset < limit && self.bytes[self.offset] == 0 {
            self.offset += 1;
        }
    }

    fn u32(&mut self, field: &'static str) -> Result<u32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u16(&mut self, field: &'static str) -> Result<u16, MdlParseError> {
        let bytes = self.bytes(2, field)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i32(&mut self, field: &'static str) -> Result<i32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f32(&mut self, field: &'static str) -> Result<f32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn vec3(&mut self, field: &'static str) -> Result<SceneVec3, MdlParseError> {
        Ok(SceneVec3 {
            x: self.f32(field)?,
            y: self.f32(field)?,
            z: self.f32(field)?,
        })
    }

    fn c_string(&mut self, field: &'static str) -> Result<String, MdlParseError> {
        let start = self.offset;
        let tail = self
            .bytes
            .get(start..)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        let len = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        let value =
            std::str::from_utf8(&tail[..len]).map_err(|_| MdlParseError::InvalidUtf8Path)?;
        self.offset = start + len + 1;
        Ok(value.replace('\\', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mdlv0023_material_and_mesh_blocks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        bytes.extend_from_slice(b"materials/puppet.json\0");
        push_u32(&mut bytes, 0);
        for value in [-1.0_f32, -2.0, 0.0, 3.0, 4.0, 0.0] {
            push_f32(&mut bytes, value);
        }
        push_u32(&mut bytes, 0x0180_000f);
        let mut vertices = Vec::new();
        push_vertex(
            &mut vertices,
            [-1.0, 0.0, 0.0],
            [0.25, 0.75],
            [3, 2, 1, 0],
            [0.5, 0.25, 0.125, 0.125],
        );
        push_vertex(
            &mut vertices,
            [1.0, 2.0, 0.0],
            [0.5, 0.125],
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
        );
        push_u32(&mut bytes, vertices.len() as u32);
        bytes.extend_from_slice(&vertices);
        push_u32(&mut bytes, 4);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());

        let model = parse_mdl_model(&bytes).expect("mdl");

        assert_eq!(model.version, 23);
        assert_eq!(model.material_paths, ["materials/puppet.json"]);
        assert_eq!(model.entries.len(), 1);
        assert_eq!(model.entries[0].entry_layout_mask, 0x0180_000f);
        assert_eq!(model.entries[0].vertices.len(), 2);
        assert_eq!(model.entries[0].vertices[0].position.x, -1.0);
        assert_eq!(model.entries[0].vertices[0].uv, [0.25, 0.75]);
        assert_eq!(model.entries[0].vertices[0].blend_indices, [3, 2, 1, 0]);
        assert_eq!(
            model.entries[0].vertices[0].blend_weights,
            [0.5, 0.25, 0.125, 0.125]
        );
        assert_eq!(model.entries[0].indices, [0, 1]);
    }

    #[test]
    fn parses_mdat_attachment_records() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"MDAT0001\0");
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&41_u16.to_le_bytes());
        bytes.extend_from_slice("eye".as_bytes());
        bytes.push(0);
        for index in 0..16 {
            let value = if index == 0 || index == 5 || index == 10 || index == 15 {
                1.0
            } else {
                0.0
            };
            push_f32(&mut bytes, value);
        }

        let model = parse_mdl_model(&bytes).expect("mdl");

        assert_eq!(model.attachments.len(), 1);
        assert_eq!(model.attachments[0].bone_index, 41);
        assert_eq!(model.attachments[0].name, "eye");
        assert_eq!(model.attachments[0].local_matrix[15], 1.0);
    }

    #[test]
    fn parses_mdls_skeleton_bone_records() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"MDLS0004");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        bytes.extend_from_slice(b"eye-bone\0");
        bytes.extend_from_slice(&3_i32.to_le_bytes());
        push_u32(&mut bytes, u32::MAX);
        push_u32(&mut bytes, 64);
        for index in 0..16 {
            let value = if index == 12 { 17.0 } else { 0.0 };
            push_f32(&mut bytes, value);
        }
        bytes.extend_from_slice(b"{}\0");

        let model = parse_mdl_model(&bytes).expect("mdl");

        assert_eq!(model.bones.len(), 1);
        assert_eq!(model.bones[0].bone_index, 0);
        assert_eq!(model.bones[0].name, "eye-bone");
        assert_eq!(model.bones[0].simulation_type, 3);
        assert_eq!(model.bones[0].parent_index, -1);
        assert_eq!(model.bones[0].local_bind_matrix[12], 17.0);
        assert_eq!(model.bones[0].simulation_json, "{}");
    }

    #[test]
    fn rejects_mdls_bone_count_beyond_remaining_payload() {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, u32::MAX);

        assert_eq!(
            parse_mdls_bones_from(&bytes, 0),
            Err(MdlParseError::UnexpectedEof("mdls_bone_records"))
        );
    }

    #[test]
    fn parses_mdla_animation_transform_tracks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"MDLS0004");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        bytes.extend_from_slice(b"eye-bone\0");
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        push_u32(&mut bytes, u32::MAX);
        push_u32(&mut bytes, 64);
        for index in 0..16 {
            push_f32(
                &mut bytes,
                f32::from(index == 0 || index == 5 || index == 10 || index == 15),
            );
        }
        bytes.extend_from_slice(b"{}\0");
        bytes.extend_from_slice(b"MDLA0006");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 2);
        push_u32(&mut bytes, 475);
        push_u32(&mut bytes, 3);
        bytes.extend_from_slice(b"blink\0");
        bytes.extend_from_slice(b"loop\0");
        push_f32(&mut bytes, 30.0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 99);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 7);
        push_u32(&mut bytes, 72);
        push_transform_sample(
            &mut bytes,
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );
        push_transform_sample(
            &mut bytes,
            [4.0, 5.0, 6.0],
            [0.0, 0.0, 1.0],
            [2.0, 2.0, 2.0],
        );
        bytes.extend_from_slice(&[0; 5]);
        push_u32(&mut bytes, 9);
        push_u32(&mut bytes, 8);
        push_f32(&mut bytes, 0.25);
        push_f32(&mut bytes, 0.75);
        bytes.extend_from_slice(&[0; 11]);
        push_u32(&mut bytes, 549);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"wave\0");
        bytes.extend_from_slice(b"loop\0");
        push_f32(&mut bytes, 30.0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 36);
        push_transform_sample(
            &mut bytes,
            [7.0, 8.0, 9.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );

        let model = parse_mdl_model(&bytes).expect("mdl");

        assert_eq!(model.animations.len(), 2);
        assert_eq!(model.animations[0].clip_id, 475);
        assert_eq!(model.animations[0].name, "blink");
        assert_eq!(model.animations[0].playback, "loop");
        assert_eq!(model.animations[0].fps, 30.0);
        assert_eq!(model.animations[0].tracks.len(), 1);
        assert_eq!(model.animations[0].tracks[0].bone_index, 0);
        assert_eq!(model.animations[0].tracks[0].track_flags, 7);
        assert_eq!(model.animations[0].tracks[0].samples.len(), 2);
        assert_eq!(model.animations[0].tracks[0].samples[1].translation.x, 4.0);
        assert_eq!(model.animations[0].tracks[0].samples[1].scale.z, 2.0);
        assert_eq!(model.animations[0].tracks[0].opacity_flags, 9);
        assert_eq!(model.animations[0].tracks[0].opacity_samples, [0.25, 0.75]);
        assert_eq!(model.animations[1].clip_id, 549);
        assert_eq!(model.animations[1].tracks[0].samples[0].translation.x, 7.0);
        assert!(model.animations[1].tracks[0].opacity_samples.is_empty());
    }

    #[test]
    fn malformed_present_mdla_section_is_not_silently_discarded() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"MDLA0006");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);

        assert!(matches!(
            parse_mdl_model(&bytes),
            Err(MdlParseError::UnexpectedEof("mdla_clip_id"))
                | Err(MdlParseError::UnexpectedEof("mdla_clip_records"))
                | Err(MdlParseError::UnexpectedEof("mdla_section_metadata"))
        ));
    }

    fn push_vertex(
        out: &mut Vec<u8>,
        position: [f32; 3],
        uv: [f32; 2],
        blend_indices: [u32; 4],
        blend_weights: [f32; 4],
    ) {
        let start = out.len();
        for value in position {
            push_f32(out, value);
        }
        out.resize(start + 40, 0);
        for value in blend_indices {
            push_u32(out, value);
        }
        for value in blend_weights {
            push_f32(out, value);
        }
        push_f32(out, uv[0]);
        push_f32(out, uv[1]);
    }

    fn push_transform_sample(
        out: &mut Vec<u8>,
        translation: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    ) {
        for value in translation.into_iter().chain(rotation).chain(scale) {
            push_f32(out, value);
        }
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(out: &mut Vec<u8>, value: f32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
}
