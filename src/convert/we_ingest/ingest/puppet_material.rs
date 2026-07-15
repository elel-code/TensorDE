//! Puppet mesh material specialization for GPU skinning shader variants.

use super::WeIrBuilder;
use crate::convert::we_ingest::ir::{
    WeIrMaterial, WeIrMaterialPass, WeIrTextureMip, WeIrUnsupported,
};
use crate::convert::we_ingest::mdl::MdlModel;
use crate::convert::we_ingest::tex::{decode_tex_upload, texture_alpha_coverage_rows};
use crate::engine::scene::SceneTextureFormat;

impl WeIrBuilder {
    pub(super) fn specialize_puppet_materials(
        &mut self,
        object: u32,
        model_path: &str,
        materials: Vec<Option<u32>>,
        model: &MdlModel,
    ) -> Vec<Option<u32>> {
        if model.bones.is_empty()
            || !model.entries.iter().any(|entry| {
                entry
                    .vertices
                    .iter()
                    .any(|vertex| vertex.blend_weights.iter().any(|weight| *weight > 1.0e-6))
            })
        {
            return materials;
        }
        materials
            .into_iter()
            .map(|material| {
                material.map(|material| self.puppet_skinning_material(object, model_path, material))
            })
            .collect()
    }

    fn puppet_skinning_material(&mut self, object: u32, model_path: &str, base_handle: u32) -> u32 {
        if let Some(handle) = self.puppet_material_by_base.get(&base_handle) {
            return *handle;
        }
        let Some(base) = self.materials.get(base_handle as usize).cloned() else {
            return base_handle;
        };
        if std::env::var_os("GILDER_CONVERT_PRESERVE_PUPPET_RGBA8").is_some() {
            self.preserve_puppet_texture_precision(object, &base);
        }
        let handle = self.materials.len() as u32;
        let pass_start = self.material_passes.len() as u32;
        let mut specialized_count = 0u32;
        for base_pass in self
            .material_passes
            .iter()
            .skip(base.pass_start as usize)
            .take(base.pass_count as usize)
            .cloned()
            .collect::<Vec<WeIrMaterialPass>>()
        {
            let mut pass = base_pass;
            pass.material = handle;
            if let Some(shader_key) = puppet_skinning_shader_key(&pass.shader_key) {
                pass.shader_key = shader_key;
                specialized_count += 1;
            }
            self.material_passes.push(pass);
        }
        if specialized_count == 0 {
            self.unsupported
                .push(crate::convert::we_ingest::ir::WeIrUnsupported {
                    object: Some(object),
                    pass_index: None,
                    feature: format!("puppet-material-has-no-skinning-shader:{model_path}"),
                    expected_subsystem: "convert/we_ingest puppet material specialization"
                        .to_owned(),
                    containment: "puppet-kept-with-unskinned-material".to_owned(),
                });
            return base_handle;
        }
        self.materials.push(WeIrMaterial {
            handle,
            resource: base.resource,
            pass_start,
            pass_count: base.pass_count,
        });
        self.puppet_material_by_base.insert(base_handle, handle);
        handle
    }

    fn preserve_puppet_texture_precision(&mut self, object: u32, material: &WeIrMaterial) {
        let mut resources = self
            .material_passes
            .iter()
            .skip(material.pass_start as usize)
            .take(material.pass_count as usize)
            .flat_map(|pass| {
                self.material_textures
                    .iter()
                    .skip(pass.texture_start as usize)
                    .take(pass.texture_count as usize)
            })
            .filter_map(|texture| texture.resource)
            .collect::<Vec<_>>();
        resources.sort_unstable();
        resources.dedup();
        for resource in resources {
            let Some(texture_index) = self
                .textures
                .iter()
                .position(|texture| texture.resource == resource)
            else {
                continue;
            };
            if !puppet_texture_requires_lossless_alpha(self.textures[texture_index].format) {
                continue;
            }
            let decoded = decode_tex_upload(&self.resources[resource as usize].payload);
            match decoded {
                Ok(upload) if upload.format == SceneTextureFormat::Rgba8Unorm => {
                    let alpha_coverage_rows = texture_alpha_coverage_rows(&upload);
                    let texture = &mut self.textures[texture_index];
                    texture.format = upload.format;
                    texture.source_runtime_format = upload.metadata.runtime_format;
                    texture.payload_format = upload.metadata.payload_format;
                    texture.sampler_flags = upload.metadata.sampler_flags;
                    texture.width = upload.metadata.width;
                    texture.height = upload.metadata.height;
                    texture.storage_width = upload.metadata.storage_width;
                    texture.storage_height = upload.metadata.storage_height;
                    texture.texv_tag = upload.metadata.texv_tag;
                    texture.texb_tag = upload.metadata.texb_tag;
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
                    texture.alpha_coverage_rows = alpha_coverage_rows;
                }
                Ok(_) => {}
                Err(source) => self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: None,
                    feature: format!(
                        "puppet-lossless-texture-decode-failed:resource{resource}:{source}"
                    ),
                    expected_subsystem: "convert/we_ingest puppet texture precision".to_owned(),
                    containment: "existing-transcoded-texture-kept".to_owned(),
                }),
            }
        }
    }
}

fn puppet_texture_requires_lossless_alpha(format: SceneTextureFormat) -> bool {
    format == SceneTextureFormat::Bc7UnormBlock
}

fn puppet_skinning_shader_key(shader_key: &str) -> Option<String> {
    let (base, suffix) = shader_key
        .split_once("__")
        .map_or((shader_key, None), |(base, suffix)| (base, Some(suffix)));
    let canonical = base
        .strip_prefix("we/")
        .unwrap_or(base)
        .to_ascii_lowercase();
    if !matches!(
        canonical.as_str(),
        "genericimage2" | "genericimage4" | "color" | "text" | "clippingmaskimage4"
    ) {
        return None;
    }
    if shader_key
        .split("__")
        .any(|part| part.eq_ignore_ascii_case("PUPPETSKINNING_1"))
    {
        return Some(shader_key.to_owned());
    }
    let mut variant = format!("{base}__PUPPETSKINNING_1");
    if let Some(suffix) = suffix {
        variant.push_str("__");
        variant.push_str(suffix);
    }
    Some(variant)
}

pub(super) fn image_effects_use_authored_texture(shader_key: &str) -> bool {
    let canonical = shader_key
        .strip_prefix("we/")
        .unwrap_or(shader_key)
        .split("__")
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(canonical.as_str(), "genericimage2" | "genericimage4")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skinning_combo_precedes_existing_mesh_shader_combos() {
        assert_eq!(
            puppet_skinning_shader_key("genericimage4__CLIPPINGTARGET_1__CLIPPINGUVS_1").as_deref(),
            Some("genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1")
        );
        assert_eq!(puppet_skinning_shader_key("effects/opacity__SLOTS_1"), None);
    }

    #[test]
    fn puppet_rgba_alpha_texture_rejects_lossy_bc7_storage() {
        assert!(puppet_texture_requires_lossless_alpha(
            SceneTextureFormat::Bc7UnormBlock
        ));
        assert!(!puppet_texture_requires_lossless_alpha(
            SceneTextureFormat::Rgba8Unorm
        ));
        assert!(!puppet_texture_requires_lossless_alpha(
            SceneTextureFormat::Bc5UnormBlock
        ));
    }
}
