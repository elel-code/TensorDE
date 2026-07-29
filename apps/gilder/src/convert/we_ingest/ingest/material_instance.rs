//! Effect-instance material specialization during WE ingest.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/effect-format.md`
//! - `reverse-engineered/gilder/docs/material-format.md`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`

use std::collections::BTreeMap;

use serde_json::Value;

use super::{bound_string, compact_json, value_i64};
use crate::convert::we_ingest::ir::{WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture};

pub(super) fn material_pass_constant_names(
    constants: &[WeIrMaterialConstant],
    pass: Option<&WeIrMaterialPass>,
) -> Vec<String> {
    let Some(pass) = pass else {
        return Vec::new();
    };
    constants
        .iter()
        .skip(pass.constant_start as usize)
        .take(pass.constant_count as usize)
        .map(|constant| constant.name.clone())
        .collect()
}

pub(super) fn merged_material_constants(
    base: &[WeIrMaterialConstant],
    instance_pass: Option<&Value>,
) -> Vec<WeIrMaterialConstant> {
    let mut constants = base
        .iter()
        .map(|constant| (constant.name.clone(), constant.value_json.clone()))
        .collect::<BTreeMap<_, _>>();
    if let Some(overrides) = instance_pass
        .and_then(|pass| pass.get("constantshadervalues"))
        .and_then(Value::as_object)
    {
        for (name, value) in overrides {
            constants.insert(name.clone(), compact_json(value));
        }
    }
    constants
        .into_iter()
        .map(|(name, value_json)| WeIrMaterialConstant { name, value_json })
        .collect()
}

pub(super) fn material_texture_bindings(
    textures: &[WeIrMaterialTexture],
    bindings: &mut BTreeMap<u32, String>,
) {
    for texture in textures.iter().filter(|texture| !texture.path.is_empty()) {
        bindings.insert(texture.slot, texture.path.clone());
    }
}

pub(super) fn file_texture_bindings(bindings: &BTreeMap<u32, String>) -> Vec<(u32, String)> {
    bindings
        .iter()
        .filter(|(_, path)| is_file_texture_binding(path))
        .map(|(slot, path)| (*slot, path.clone()))
        .collect()
}

fn is_file_texture_binding(path: &str) -> bool {
    !path.is_empty()
        && !matches!(
            path,
            "previous" | "_previous" | "$previous" | "source" | "g_Texture0"
        )
        && !path.starts_with("_rt_")
        && !path.starts_with("_alias_")
        && !path.starts_with("fbo_")
}

pub(super) fn push_instance_texture_overrides(
    instance_pass: &Value,
    bindings: &mut BTreeMap<u32, String>,
) {
    for (slot, texture) in instance_pass
        .get("textures")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(path) = bound_string(Some(texture)) {
            bindings.insert(slot as u32, path);
        } else if slot == 0 {
            bindings.insert(0, "previous".to_owned());
        }
    }
}

pub(super) fn push_instance_combo_overrides(
    instance_pass: &Value,
    combos: &mut BTreeMap<String, i64>,
) {
    if let Some(instance_combos) = instance_pass.get("combos").and_then(Value::as_object) {
        for (name, value) in instance_combos {
            if let Some(value) = value_i64(Some(value)) {
                combos.insert(name.clone(), value);
            }
        }
    }
}

pub(super) fn effect_shader_variant_key(
    shader: &str,
    bindings: &BTreeMap<u32, String>,
    combos: &BTreeMap<String, i64>,
    combo_defaults: &BTreeMap<String, i64>,
) -> String {
    let authored_base = shader.split("__").next().unwrap_or(shader);
    let base = canonical_effect_shader_program(authored_base);
    let texture_slot_mask = bindings
        .keys()
        .copied()
        .filter(|slot| *slot < 32)
        .fold(0u32, |mask, slot| mask | (1u32 << slot));
    let mut key = format!("{base}__SLOTS_{texture_slot_mask:x}");
    if base.eq_ignore_ascii_case("effects/iris")
        && texture_slot_mask & (1 << 1) != 0
        && !combos.keys().any(|name| name.eq_ignore_ascii_case("MASK"))
    {
        key.push_str("__MASK_1");
    }
    for (name, value) in combos.iter().filter(|(name, value)| {
        !name.eq_ignore_ascii_case("SLOTS")
            && combo_default(combo_defaults, name).unwrap_or(0) != **value
    }) {
        key.push_str("__");
        key.push_str(&name.to_ascii_uppercase());
        key.push('_');
        key.push_str(&value.to_string());
    }
    key
}

fn canonical_effect_shader_program(shader: &str) -> String {
    if let Some(program) = shader.strip_prefix("shaders/effects/") {
        return format!("effects/{}", program.to_ascii_lowercase());
    }
    if let Some(program) = shader.strip_prefix("effects/") {
        return format!("effects/{}", program.to_ascii_lowercase());
    }
    shader.to_owned()
}

fn combo_default(defaults: &BTreeMap<String, i64>, name: &str) -> Option<i64> {
    defaults
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::we_ingest::ingest_wallpaper_engine_project;
    use crate::convert::we_ingest::ir::{WeIrImageTargetRole, WeIrShaderOrigin};
    use crate::engine::render_graph::RenderPassRole;
    use std::fs;

    fn scene_package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(&8u32.to_le_bytes());
        header.extend_from_slice(b"PKGV0024");
        header.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut data = Vec::new();
        for (path, payload) in entries {
            header.extend_from_slice(&(path.len() as u32).to_le_bytes());
            header.extend_from_slice(path.as_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            data.extend_from_slice(payload);
        }
        header.extend(data);
        header
    }

    #[test]
    fn effect_variant_key_preserves_sparse_slots_and_nonzero_combos() {
        let bindings = [
            (0, "previous".to_owned()),
            (2, "effects/waterripplenormal".to_owned()),
        ]
        .into_iter()
        .collect();
        let combos = [("MASK".to_owned(), 0), ("DUALWAVES".to_owned(), 1)]
            .into_iter()
            .collect();

        assert_eq!(
            effect_shader_variant_key("effects/waterwaves", &bindings, &combos, &BTreeMap::new(),),
            "effects/waterwaves__SLOTS_5__DUALWAVES_1"
        );
    }

    #[test]
    fn authored_effect_variant_key_preserves_package_program_identity() {
        let bindings = [(0, "previous".to_owned())].into_iter().collect();
        let combos = [("SHAPE".to_owned(), 7)].into_iter().collect();

        assert_eq!(
            effect_shader_variant_key(
                "workshop/test/effects/Simple_Audio_Bars",
                &bindings,
                &combos,
                &BTreeMap::new(),
            ),
            "workshop/test/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7"
        );
    }

    #[test]
    fn iris_mask_texture_selects_the_mask_shader_combo() {
        let bindings = [(0, "previous".to_owned()), (1, "masks/iris".to_owned())]
            .into_iter()
            .collect();

        assert_eq!(
            effect_shader_variant_key(
                "effects/iris",
                &bindings,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            "effects/iris__SLOTS_3__MASK_1"
        );
    }

    #[test]
    fn explicit_zero_is_canonical_when_shader_default_is_nonzero() {
        let bindings = [(0, "previous".to_owned())].into_iter().collect();
        let combos = [
            ("B_SQUARE".to_owned(), 0),
            ("C_ALPHA_ONLY".to_owned(), 0),
            ("SOFT".to_owned(), 1),
        ]
        .into_iter()
        .collect();
        let defaults = [
            ("B_SQUARE".to_owned(), 1),
            ("C_ALPHA_ONLY".to_owned(), 1),
            ("SOFT".to_owned(), 0),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            effect_shader_variant_key("effects/rounded_mask", &bindings, &combos, &defaults),
            "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1"
        );
    }

    #[test]
    fn effect_program_contract_preserves_authored_package_paths() {
        let bindings = [(0, "previous".to_owned())].into_iter().collect();
        let combos = [
            ("AA_CATEGORY".to_owned(), 1),
            ("BLENDMODE".to_owned(), 20),
            ("STEPANIM".to_owned(), 1),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            effect_shader_variant_key(
                "workshop/current/effects/procedural_noise",
                &bindings,
                &combos,
                &BTreeMap::new(),
            ),
            "workshop/current/effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__BLENDMODE_20__STEPANIM_1"
        );
        assert_eq!(
            effect_shader_variant_key(
                "workshop/current/effects/opacity",
                &bindings,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            "workshop/current/effects/opacity__SLOTS_1"
        );
    }

    #[test]
    fn unrelated_shader_basename_is_not_guessed_as_an_effect_contract() {
        let bindings = [(0, "previous".to_owned())].into_iter().collect();

        assert_eq!(
            effect_shader_variant_key(
                "workshop/current/custom/procedural_noise",
                &bindings,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            "workshop/current/custom/procedural_noise__SLOTS_1"
        );
    }

    #[test]
    fn file_texture_bindings_exclude_graph_targets() {
        let bindings = [
            (0, "previous".to_owned()),
            (1, "masks/custom".to_owned()),
            (2, "_rt_Effect".to_owned()),
            (3, "util/perlin_256".to_owned()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            file_texture_bindings(&bindings),
            vec![
                (1, "masks/custom".to_owned()),
                (3, "util/perlin_256".to_owned()),
            ]
        );
    }

    #[test]
    fn instance_constants_override_base_values_without_losing_names() {
        let base = vec![
            WeIrMaterialConstant {
                name: "speed".to_owned(),
                value_json: "1.0".to_owned(),
            },
            WeIrMaterialConstant {
                name: "rough".to_owned(),
                value_json: "0.2".to_owned(),
            },
        ];
        let instance = serde_json::json!({
            "constantshadervalues": {"speed": 2.5, "phase": 0.25}
        });

        let merged = merged_material_constants(&base, Some(&instance));

        assert_eq!(
            merged,
            vec![
                WeIrMaterialConstant {
                    name: "phase".to_owned(),
                    value_json: "0.25".to_owned(),
                },
                WeIrMaterialConstant {
                    name: "rough".to_owned(),
                    value_json: "0.2".to_owned(),
                },
                WeIrMaterialConstant {
                    name: "speed".to_owned(),
                    value_json: "2.5".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn ingest_materializes_effect_instance_values_and_variant_in_ir() {
        let root = std::env::temp_dir().join(format!(
            "gilder-we-effect-material-instance-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        for directory in [
            "models",
            "materials",
            "materials/effects",
            "effects/opacity",
        ] {
            fs::create_dir_all(root.join(directory)).expect("test directory");
        }
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json","title":"Effect"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":7,"image":"models/layer.json","effects":[{"file":"effects/opacity/effect.json","passes":[{"constantshadervalues":{"alpha":0.37}}]}]}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("models/layer.json"),
            r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
        )
        .expect("model");
        fs::write(
            root.join("materials/layer.json"),
            r#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
        )
        .expect("base material");
        fs::write(
            root.join("effects/opacity/effect.json"),
            r#"{"passes":[{"material":"materials/effects/opacity.json"}]}"#,
        )
        .expect("effect");
        fs::write(
            root.join("materials/effects/opacity.json"),
            r#"{"passes":[{"shader":"effects/opacity"}]}"#,
        )
        .expect("effect material");

        let ir = ingest_wallpaper_engine_project(&root).expect("effect IR");
        let effect_pass = ir.render_graphs[0]
            .passes
            .iter()
            .find(|pass| pass.role == RenderPassRole::EffectMaterial)
            .expect("effect graph pass");
        let material = &ir.materials[effect_pass.material_index.expect("instance material")];
        let pass = &ir.material_passes[material.pass_start as usize];
        let constant = &ir.material_constants[pass.constant_start as usize];

        assert_eq!(pass.shader_key, "effects/opacity__SLOTS_1");
        assert_eq!(pass.shader_source_key, "effects/opacity");
        assert_eq!(pass.shader_origin, WeIrShaderOrigin::EngineBuiltIn);
        assert_eq!(constant.name, "alpha");
        assert_eq!(constant.value_json, "0.37");
        assert!(
            ir.shader_contracts
                .iter()
                .any(|contract| contract.shader_key == "effects/opacity__SLOTS_1")
        );
        assert!(
            ir.shader_contracts
                .iter()
                .all(|contract| contract.shader_key != "effects/opacity")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_materializes_shader_declared_full_framebuffer_binding() {
        let root = std::env::temp_dir().join(format!(
            "gilder-we-shader-runtime-target-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test directory");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json","title":"Framebuffer input"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.pkg"),
            scene_package(&[
                (
                    "scene.json",
                    br#"{"objects":[{"id":7,"image":"models/util/composelayer.json","effects":[{"file":"effects/oscilloscope/effect.json","passes":[{"combos":{"RESOLUTION":16}}]}]}]}"#,
                ),
                (
                    "effects/oscilloscope/effect.json",
                    br#"{"passes":[{"material":"materials/effects/oscilloscope.json"}]}"#,
                ),
                (
                    "materials/effects/oscilloscope.json",
                    br#"{"passes":[{"shader":"workshop/test/effects/audio_responsive_oscilloscope","blending":"normal"}]}"#,
                ),
                (
                    "shaders/workshop/test/effects/audio_responsive_oscilloscope.vert",
                    br#"// [COMBO] {"combo":"RESOLUTION","type":"options","default":32}
attribute vec3 a_Position;"#,
                ),
                (
                    "shaders/workshop/test/effects/audio_responsive_oscilloscope.frag",
                    br#"// [COMBO] {"combo":"RESOLUTION","type":"options","default":32}
uniform sampler2D g_Texture0; // {"material":"framebuffer","hidden":true}
uniform sampler2D g_Texture2; // {"default":"_rt_FullFrameBuffer","hidden":true,"material":"backgroundTexture"}"#,
                ),
            ]),
        )
        .expect("scene package");

        let ir = ingest_wallpaper_engine_project(&root).expect("effect IR");
        let terminal = ir.render_graphs[0].passes.last().expect("terminal pass");

        assert_eq!(terminal.role, RenderPassRole::SceneComposite);
        assert_eq!(
            terminal.shader.as_deref(),
            Some("workshop/test/effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16")
        );
        let contract = ir
            .shader_contracts
            .iter()
            .find(|contract| {
                contract.shader_key
                    == "workshop/test/effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16"
            })
            .expect("workshop shader contract");
        assert_eq!(contract.origin, WeIrShaderOrigin::AuthoredPackage);
        assert_eq!(
            contract.shader_source_key,
            "workshop/test/effects/audio_responsive_oscilloscope"
        );
        assert!(terminal.bindings.contains(
            &crate::engine::render_graph::TextureBindingRole::PreviousGraphTarget { slot: 0 }
        ));
        assert!(terminal.bindings.contains(
            &crate::engine::render_graph::TextureBindingRole::EffectTarget {
                slot: 2,
                name: "_rt_FullFrameBuffer".to_owned(),
            }
        ));
        assert!(ir.image_targets.iter().any(|target| {
            target.name == "_rt_FullFrameBuffer"
                && target.role == WeIrImageTargetRole::FirstClassEffectTarget
        }));

        let _ = fs::remove_dir_all(root);
    }
}
