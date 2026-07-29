//! Authored shader provenance derived from Wallpaper Engine program paths.
//!
//! The classification follows the installed-asset versus Workshop package
//! boundary documented under `reverse-engineered/gilder/`. A shared basename
//! never turns a Workshop shader into an engine-owned program.

use super::ir::WeIrShaderOrigin;

include!(concat!(env!("OUT_DIR"), "/gilder_scene_shader_origins.rs"));

pub(super) fn scene_shader_origin(authored_key: &str) -> WeIrShaderOrigin {
    let program = authored_key
        .split_once("__")
        .map_or(authored_key, |(program, _)| program);
    let program = program.strip_prefix("shaders/").unwrap_or(program);
    if program.starts_with("we/")
        || program.starts_with("gilder/")
        || is_engine_builtin_effect_program(program)
        || is_engine_core_program(program)
    {
        WeIrShaderOrigin::EngineBuiltIn
    } else {
        WeIrShaderOrigin::AuthoredPackage
    }
}

fn is_engine_core_program(program: &str) -> bool {
    matches!(
        program,
        "generic4"
            | "genericimage2"
            | "genericimage4"
            | "genericparticle"
            | "clippingmaskimage4"
            | "color"
            | "text"
            | "composelayer"
            | "flat"
            | "minimalalpha"
            | "passthrough"
            | "utilitycomposite"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_engine_namespaces_are_built_in() {
        for key in [
            "genericimage4",
            "effects/tint__SLOTS_1",
            "shaders/effects/blur_gaussian",
            "we/passthrough",
            "gilder/dynamic-text",
        ] {
            assert_eq!(scene_shader_origin(key), WeIrShaderOrigin::EngineBuiltIn);
        }
    }

    #[test]
    fn workshop_effects_remain_authored_despite_shared_basenames() {
        for key in [
            "workshop/3082978660/effects/Simple_Audio_Bars",
            "shaders/workshop/2790231929/effects/waterripple__SLOTS_5",
            "workshop/3165346237/effects/lut_loader",
            "custom/my_shader",
        ] {
            assert_eq!(scene_shader_origin(key), WeIrShaderOrigin::AuthoredPackage);
        }
    }

    #[test]
    fn direct_package_only_effects_do_not_borrow_builtin_identity() {
        for key in [
            "effects/111__SLOTS_1__BLENDMODE_7",
            "effects/huan__SLOTS_1",
            "effects/qiu__SLOTS_1",
            "effects/rounded_mask__SLOTS_1__SOFT_1",
        ] {
            assert_eq!(scene_shader_origin(key), WeIrShaderOrigin::AuthoredPackage);
        }
    }
}
