//! Typed Wallpaper Engine built-in utility-layer assets.
//!
//! These canonical assets live in Wallpaper Engine's shared asset bundle, so
//! workshop `scene.pkg` files reference them without embedding their payloads.
//!
//! References:
//! - `reverse-engineered/gilder/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/gilder/docs/exe/blend-and-render.md`
//! - `reverse-engineered/gilder/docs/javascript-api.md`

use crate::convert::we_ingest::ir::WeIrUtilityLayerKind;

pub(super) const FULL_FRAMEBUFFER_TARGET: &str = "_rt_FullFrameBuffer";

const COMPOSITE_MODEL: &[u8] =
    br#"{"material":"materials/util/composelayer.json","passthrough":true}"#;
const COMPOSITE_MATERIAL: &[u8] = br#"{"passes":[{"shader":"composelayer","depthtest":"disabled","depthwrite":"disabled","blending":"translucent","cullmode":"nocull","textures":["_rt_FullFrameBuffer"]}]}"#;
const FULLSCREEN_MODEL: &[u8] =
    br#"{"material":"materials/util/fullscreenlayer.json","fullscreen":true,"passthrough":true}"#;
const FULLSCREEN_MATERIAL: &[u8] = br#"{"passes":[{"shader":"passthrough","depthtest":"disabled","depthwrite":"disabled","blending":"translucent","cullmode":"nocull","textures":["_rt_FullFrameBuffer"]}]}"#;
const SOLID_MODEL: &[u8] = br#"{"material":"materials/util/solidlayer.json","solidlayer":true}"#;
const SOLID_MATERIAL: &[u8] = br#"{"passes":[{"shader":"flat","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled","blending":"translucent"}]}"#;

pub(super) fn utility_layer_kind(path: &str) -> Option<WeIrUtilityLayerKind> {
    match path {
        "models/util/composelayer.json" | "models/util/composelayer_depthtest.json" => {
            Some(WeIrUtilityLayerKind::FramebufferComposite)
        }
        "models/util/fullscreenlayer.json" => Some(WeIrUtilityLayerKind::FullscreenPostprocess),
        "models/util/solidlayer.json" | "models/util/solidlayer_depthtest.json" => {
            Some(WeIrUtilityLayerKind::SolidColor)
        }
        _ => None,
    }
}

pub(super) fn builtin_utility_asset(path: &str) -> Option<&'static [u8]> {
    match path {
        "models/util/composelayer.json" | "models/util/composelayer_depthtest.json" => {
            Some(COMPOSITE_MODEL)
        }
        "materials/util/composelayer.json" | "materials/util/composelayer_depthtest.json" => {
            Some(COMPOSITE_MATERIAL)
        }
        "models/util/fullscreenlayer.json" => Some(FULLSCREEN_MODEL),
        "materials/util/fullscreenlayer.json" => Some(FULLSCREEN_MATERIAL),
        "models/util/solidlayer.json" | "models/util/solidlayer_depthtest.json" => {
            Some(SOLID_MODEL)
        }
        "materials/util/solidlayer.json" | "materials/util/solidlayer_depthtest.json" => {
            Some(SOLID_MATERIAL)
        }
        _ => None,
    }
}

pub(super) fn is_runtime_render_target(path: &str) -> bool {
    path.starts_with("_rt_") || path.starts_with("_alias_") || path.starts_with("fbo_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::we_ingest::ingest_wallpaper_engine_project;
    use std::fs;

    #[test]
    fn canonical_utility_paths_are_typed_without_workshop_payloads() {
        assert_eq!(
            utility_layer_kind("models/util/composelayer.json"),
            Some(WeIrUtilityLayerKind::FramebufferComposite)
        );
        assert_eq!(
            utility_layer_kind("models/util/fullscreenlayer.json"),
            Some(WeIrUtilityLayerKind::FullscreenPostprocess)
        );
        assert!(builtin_utility_asset("materials/util/solidlayer.json").is_some());
        assert!(is_runtime_render_target(FULL_FRAMEBUFFER_TARGET));
    }

    #[test]
    fn fullscreen_layer_without_effects_records_no_framebuffer_work() {
        let root = std::env::temp_dir().join(format!(
            "gilder-we-utility-layer-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test root");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":7,"image":"models/util/fullscreenlayer.json"}]}"#,
        )
        .expect("scene");

        let ir = ingest_wallpaper_engine_project(&root).expect("utility IR");

        assert_eq!(
            ir.objects[0].utility_layer,
            Some(WeIrUtilityLayerKind::FullscreenPostprocess)
        );
        assert!(
            ir.resources
                .iter()
                .filter(|resource| resource.source
                    == crate::convert::we_ingest::ir::WeIrResourceSource::Builtin)
                .count()
                >= 2
        );
        assert!(ir.render_graphs[0].passes.is_empty());
        assert!(ir.image_targets.is_empty());
        assert!(ir.unsupported.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
