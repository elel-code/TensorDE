//! Lower parsed MDL meshes, materials, puppets, and animation clips into ingest IR.

use crate::convert::we_ingest::ir::*;
use crate::convert::we_ingest::mdl::{MdlAnimationClip, MdlAttachment, MdlBone, parse_mdl_model};

use super::{WeIngestError, WeIrBuilder};

impl WeIrBuilder {
    pub(super) fn add_mdl_model(
        &mut self,
        object: u32,
        image_path: &str,
        resource: Option<u32>,
    ) -> Result<Option<u32>, WeIngestError> {
        let Some(resource) = resource else {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("missing-mdl-resource:{image_path}"),
                expected_subsystem: "convert/we_ingest asset source".to_owned(),
                containment: "object-kept-without-resource".to_owned(),
            });
            return Ok(None);
        };
        let payload = self.resources[resource as usize].payload.clone();
        let model = match parse_mdl_model(&payload) {
            Ok(model) => model,
            Err(err) => {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: None,
                    feature: format!("mdl-parse-failed:{image_path}:{err}"),
                    expected_subsystem: "convert/we_ingest MDLV0023 mesh parser".to_owned(),
                    containment: "object-kept-without-mdl-mesh".to_owned(),
                });
                return Ok(None);
            }
        };
        let materials = self.add_mdl_materials(object, image_path, &model.material_paths)?;
        let materials = self.specialize_puppet_materials(object, image_path, materials, &model);
        let material = materials.first().copied().flatten();
        let mut clipping_mask_resources = Vec::with_capacity(model.entries.len());
        for (entry_index, entry) in model.entries.iter().enumerate() {
            let material_path = model
                .material_paths
                .get(entry_index)
                .or_else(|| model.material_paths.first())
                .map(String::as_str);
            let mut entry_resources = Vec::with_capacity(entry.clipping_subdraws.len());
            for subdraw in &entry.clipping_subdraws {
                entry_resources.push(self.add_texture(&subdraw.mask_resource, material_path)?);
            }
            clipping_mask_resources.push(entry_resources);
        }
        let (mesh_start, mesh_count) = self.add_mdl_meshes(
            object,
            image_path,
            &model.entries,
            &materials,
            &clipping_mask_resources,
        );
        self.add_mdl_puppet(
            object,
            resource,
            mesh_start,
            mesh_count,
            &model.bones,
            &model.attachments,
            &model.animations,
        );
        Ok(material)
    }

    fn add_mdl_materials(
        &mut self,
        object: u32,
        image_path: &str,
        material_paths: &[String],
    ) -> Result<Vec<Option<u32>>, WeIngestError> {
        if material_paths.is_empty() {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("mdl-has-no-material-paths:{image_path}"),
                expected_subsystem: "convert/we_ingest MDLV0023 material table".to_owned(),
                containment: "mdl-meshes-kept-without-material".to_owned(),
            });
            return Ok(Vec::new());
        }

        material_paths
            .iter()
            .map(|path| self.add_material(path).map(Some))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn add_mdl_puppet(
        &mut self,
        object: u32,
        resource: u32,
        mesh_start: u32,
        mesh_count: u32,
        bones: &[MdlBone],
        attachments: &[MdlAttachment],
        animations: &[MdlAnimationClip],
    ) {
        let puppet = self.puppets.len() as u32;
        let bone_start = self.puppet_bones.len() as u32;
        let attachment_start = self.puppet_attachments.len() as u32;
        for bone in bones {
            self.puppet_bones.push(WeIrPuppetBone {
                puppet,
                bone_index: bone.bone_index,
                name: bone.name.clone(),
                simulation_type: bone.simulation_type,
                parent_index: bone.parent_index,
                local_bind_matrix: bone.local_bind_matrix,
                simulation_json: bone.simulation_json.clone(),
            });
        }
        for attachment in attachments {
            self.puppet_attachments.push(WeIrPuppetAttachment {
                puppet,
                bone_index: attachment.bone_index,
                name: attachment.name.clone(),
                local_matrix: attachment.local_matrix,
            });
        }
        let clip_start = self.puppet_animation_clips.len() as u32;
        for animation in animations {
            self.push_mdl_puppet_animation(puppet, animation);
        }
        self.puppets.push(WeIrPuppet {
            object,
            resource,
            mesh_start,
            mesh_count,
            bone_start,
            bone_count: self.puppet_bones.len() as u32 - bone_start,
            attachment_start,
            attachment_count: self.puppet_attachments.len() as u32 - attachment_start,
        });
        let clip_count = self.puppet_animation_clips.len() as u32 - clip_start;
        if clip_count != 0 && bones.is_empty() {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: "mdla-animation-without-mdls-bone-table".to_owned(),
                expected_subsystem: "convert/we_ingest MDLA0006 animation lowering".to_owned(),
                containment: "animation-records-kept-with-track-ordinal-bone-indices".to_owned(),
            });
        }
    }

    fn push_mdl_puppet_animation(&mut self, puppet: u32, animation: &MdlAnimationClip) {
        let clip = self.puppet_animation_clips.len() as u32;
        let track_start = self.puppet_animation_tracks.len() as u32;
        for track in &animation.tracks {
            let sample_start = self.puppet_animation_transform_samples.len() as u32;
            let opacity_sample_start = self.puppet_animation_opacity_samples.len() as u32;
            self.puppet_animation_transform_samples
                .extend(
                    track
                        .samples
                        .iter()
                        .map(|sample| WeIrPuppetAnimationTransformSample {
                            translation: sample.translation,
                            rotation: sample.rotation,
                            scale: sample.scale,
                        }),
                );
            self.puppet_animation_opacity_samples
                .extend(track.opacity_samples.iter().copied());
            self.puppet_animation_tracks.push(WeIrPuppetAnimationTrack {
                clip,
                bone_index: track.bone_index,
                track_flags: track.track_flags,
                sample_start,
                sample_count: self.puppet_animation_transform_samples.len() as u32 - sample_start,
                opacity_flags: track.opacity_flags,
                opacity_sample_start,
                opacity_sample_count: self.puppet_animation_opacity_samples.len() as u32
                    - opacity_sample_start,
            });
        }
        self.puppet_animation_clips.push(WeIrPuppetAnimationClip {
            puppet,
            clip_id: animation.clip_id,
            flags: animation.flags,
            name: animation.name.clone(),
            playback: animation.playback.clone(),
            fps: animation.fps,
            frame_count: animation.frame_count,
            frame_metadata: animation.frame_metadata,
            track_start,
            track_count: self.puppet_animation_tracks.len() as u32 - track_start,
        });
    }
}
