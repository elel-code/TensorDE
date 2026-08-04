//! Semantic texture-storage policies that require material-use context.

use super::*;
use crate::engine::scene::SceneTextureFormat;

impl WeIrBuilder {
    /// WE uploads decoded RGBA particle sprites without another lossy block-
    /// compression pass. Preserve that single-sample alpha/color contract after
    /// the generic texture loader has discovered that the material is a particle.
    pub(in super::super) fn preserve_particle_material_rgba(
        &mut self,
        material: u32,
    ) -> Result<(), WeIngestError> {
        let material = self.materials.get(material as usize).ok_or_else(|| {
            WeIngestError::InvalidProject("particle material is missing".to_owned())
        })?;
        let texture_resources = self.material_passes
            [material.pass_start as usize..(material.pass_start + material.pass_count) as usize]
            .iter()
            .flat_map(|pass| {
                self.material_textures[pass.texture_start as usize
                    ..(pass.texture_start + pass.texture_count) as usize]
                    .iter()
            })
            .filter_map(|binding| {
                binding
                    .resource
                    .map(|resource| (resource, binding.path.clone()))
            })
            .collect::<Vec<_>>();

        for (resource, path) in texture_resources {
            let Some(texture_index) = self
                .textures
                .iter()
                .position(|texture| texture.resource == resource)
            else {
                continue;
            };
            if self.textures[texture_index].source_runtime_format != 0
                || self.textures[texture_index].format != SceneTextureFormat::Bc7UnormBlock
            {
                continue;
            }
            let upload = decode_tex_upload(&self.resources[resource as usize].payload)
                .map_err(|source| WeIngestError::Tex { path, source })?;
            if upload.format != SceneTextureFormat::Rgba8Unorm {
                return Err(WeIngestError::InvalidProject(format!(
                    "particle texture resource {resource} reports RGBA source format but decodes as {:?}",
                    upload.format
                )));
            }
            let texture = &mut self.textures[texture_index];
            texture.format = upload.format;
            texture.storage_width = upload.metadata.storage_width;
            texture.storage_height = upload.metadata.storage_height;
            texture.mips = upload
                .mips
                .into_iter()
                .map(|mip| WeIrTextureMip {
                    width: mip.width,
                    height: mip.height,
                    payload_offset: mip.payload_offset,
                    payload_len: mip.payload_len,
                })
                .collect();
            texture.upload_payload = upload.payload;
        }
        Ok(())
    }
}
