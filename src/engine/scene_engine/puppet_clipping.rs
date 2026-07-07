//! WE puppet clipping program facts owned by the scene engine.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/storage/`

use serde::Serialize;

use crate::core::scene::SceneMeshPuppetClippingRecord as SourcePuppetClippingRecord;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScenePuppetClippingProgram {
    pub records: Vec<ScenePuppetClippingRecord>,
    pub bone_indices: Vec<u32>,
    pub frame_keys: Vec<u32>,
    pub active_sources: Vec<ScenePuppetClippingActiveSource>,
}

impl ScenePuppetClippingProgram {
    pub fn from_source_records(records: Vec<SourcePuppetClippingRecord>) -> Self {
        let mut program = Self::default();
        for record in records {
            program.push_source_record(record);
        }
        program
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
            && self.bone_indices.is_empty()
            && self.frame_keys.is_empty()
            && self.active_sources.is_empty()
    }

    fn push_source_record(&mut self, record: SourcePuppetClippingRecord) {
        let first_bone = saturating_u32(self.bone_indices.len());
        self.bone_indices
            .extend(record.bones.into_iter().map(saturating_u32));
        let bone_count = saturating_u32(self.bone_indices.len()).saturating_sub(first_bone);

        let first_frame_key = saturating_u32(self.frame_keys.len());
        self.frame_keys.extend(record.frame_keys);
        let frame_key_count = saturating_u32(self.frame_keys.len()).saturating_sub(first_frame_key);

        let source_name_hash = record
            .source_name
            .as_deref()
            .map(scene_stable_name_hash)
            .unwrap_or_default();

        self.records.push(ScenePuppetClippingRecord {
            source_name: record.source_name,
            source_name_hash,
            mask: record.mask,
            mask_resource: record.mask_resource,
            duration_frames: record.duration_frames,
            flags: record.flags,
            first_bone,
            bone_count,
            first_frame_key,
            frame_key_count,
            active_source_index: None,
            mask_texture_index: None,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenePuppetClippingRecord {
    pub source_name: Option<String>,
    pub source_name_hash: u64,
    pub mask: String,
    pub mask_resource: Option<String>,
    pub duration_frames: u32,
    pub flags: u32,
    pub first_bone: u32,
    pub bone_count: u32,
    pub first_frame_key: u32,
    pub frame_key_count: u32,
    pub active_source_index: Option<u32>,
    pub mask_texture_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenePuppetClippingActiveSource {
    pub source_name: String,
    pub scalar_bits: u32,
    pub source_scale: u32,
    pub flags: u32,
    pub transform_index: u32,
    pub parameter0: f32,
    pub parameter1: f32,
}

pub fn scene_stable_name_hash(name: &str) -> u64 {
    name.as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_program_flattens_source_records_for_gpu_storage() {
        let program =
            ScenePuppetClippingProgram::from_source_records(vec![SourcePuppetClippingRecord {
                source_name: Some("eye-right".to_owned()),
                mask: "masks/clipping_mask_eye".to_owned(),
                mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                duration_frames: 1680,
                flags: 1,
                bones: vec![42, 43],
                frame_keys: vec![0, 1, 2],
            }]);

        assert_eq!(program.records.len(), 1);
        assert_eq!(program.bone_indices, vec![42, 43]);
        assert_eq!(program.frame_keys, vec![0, 1, 2]);
        assert_eq!(program.records[0].source_name.as_deref(), Some("eye-right"));
        assert_eq!(program.records[0].first_bone, 0);
        assert_eq!(program.records[0].bone_count, 2);
        assert_eq!(program.records[0].first_frame_key, 0);
        assert_eq!(program.records[0].frame_key_count, 3);
        assert_eq!(
            program.records[0].source_name_hash,
            scene_stable_name_hash("eye-right")
        );
    }
}
