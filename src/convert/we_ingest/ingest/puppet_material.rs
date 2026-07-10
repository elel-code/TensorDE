//! Puppet mesh material specialization for GPU skinning shader variants.

use super::WeIrBuilder;
use crate::convert::we_ingest::ir::{WeIrMaterial, WeIrMaterialPass};
use crate::convert::we_ingest::mdl::MdlModel;

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
}
