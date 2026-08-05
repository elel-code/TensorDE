//! Retained source-program evidence for semantically proven final-effect fusions.

use super::*;

impl WeIrBuilder {
    pub(super) fn retain_fused_authored_source_materials(
        &mut self,
        base_material: u32,
        effects: &[WeEffectPassContract],
    ) {
        let source_materials = std::iter::once(base_material as usize)
            .chain(effects.iter().filter_map(|effect| effect.material_index))
            .collect::<Vec<_>>();
        for material_index in source_materials {
            let authored_package = self.materials.get(material_index).is_some_and(|material| {
                self.material_passes[material.pass_start as usize
                    ..(material.pass_start + material.pass_count) as usize]
                    .iter()
                    .any(|pass| pass.shader_origin == WeIrShaderOrigin::AuthoredPackage)
            });
            if authored_package
                && !self
                    .fused_authored_source_materials
                    .contains(&material_index)
            {
                self.fused_authored_source_materials.push(material_index);
            }
        }
    }
}
