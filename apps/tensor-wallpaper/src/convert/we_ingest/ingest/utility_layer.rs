//! Typed Wallpaper Engine built-in utility-layer assets.
//!
//! These canonical assets live in Wallpaper Engine's shared asset bundle, so
//! workshop `scene.pkg` files reference them without embedding their payloads.
//!
//! References:
//! - `reverse-engineered/tensor-wallpaper/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/blend-and-render.md`
//! - `reverse-engineered/tensor-wallpaper/docs/javascript-api.md`

use crate::convert::we_ingest::ir::WeIrUtilityLayerKind;

pub(super) const FULL_FRAMEBUFFER_TARGET: &str = "_rt_FullFrameBuffer";

const COMPOSITE_MODEL: &[u8] =
    br#"{"material":"materials/util/composelayer.json","passthrough":true}"#;
const PROJECT_MODEL: &[u8] = br#"{"material":"materials/util/composelayer.json","passthrough":true,"autosize":true,"projectlayer":true}"#;
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
        "models/util/projectlayer.json" => Some(WeIrUtilityLayerKind::ProjectLayer),
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
        "models/util/projectlayer.json" => Some(PROJECT_MODEL),
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
        assert_eq!(
            utility_layer_kind("models/util/projectlayer.json"),
            Some(WeIrUtilityLayerKind::ProjectLayer)
        );
        assert!(
            WeIrUtilityLayerKind::ProjectLayer.samples_scene_color(),
            "a project layer consumes the scene framebuffer"
        );
        assert!(
            WeIrUtilityLayerKind::FramebufferComposite.samples_scene_color(),
            "a composelayer consumes an explicit SceneColor snapshot"
        );
        assert!(
            !WeIrUtilityLayerKind::FramebufferComposite.uses_physical_graph_source(),
            "a composelayer's object-local targets keep its authored extent"
        );
        assert!(
            WeIrUtilityLayerKind::ProjectLayer.uses_physical_graph_source(),
            "a project layer's local targets follow the physical scene surface"
        );
        assert!(builtin_utility_asset("models/util/projectlayer.json").is_some());
        assert!(builtin_utility_asset("materials/util/solidlayer.json").is_some());
        assert!(is_runtime_render_target(FULL_FRAMEBUFFER_TARGET));
    }

    #[test]
    fn composelayer_keeps_local_target_seed_authored_while_snapshot_stays_physical() {
        let root = std::env::temp_dir().join(format!(
            "tensor-wallpaper-we-composelayer-target-domain-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("effects/post")).expect("effect directory");
        fs::create_dir_all(root.join("materials/effects")).expect("material directory");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":7,"image":"models/util/composelayer.json","size":"1000 1000","effects":[{"file":"effects/post/effect.json","id":8}]}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("effects/post/effect.json"),
            r#"{"fbos":[{"name":"_rt_HalfSource","format":"rgba_backbuffer","scale":2}],"passes":[{"material":"materials/effects/post.json","target":"_rt_HalfSource","bind":[{"index":0,"target":"previous"}]},{"material":"materials/effects/post.json","bind":[{"index":0,"target":"_rt_HalfSource"}]}]}"#,
        )
        .expect("effect");
        fs::write(
            root.join("materials/effects/post.json"),
            r#"{"passes":[{"shader":"passthrough","blending":"normal"}]}"#,
        )
        .expect("effect material");

        let ir = ingest_wallpaper_engine_project(&root).expect("composelayer IR");
        assert_eq!(
            ir.objects[0].utility_layer,
            Some(WeIrUtilityLayerKind::FramebufferComposite)
        );
        assert_eq!(
            ir.objects[0].render_source_extent_domain,
            crate::convert::we_ingest::ir::WeIrRenderSourceExtentDomain::OwnerAuthored
        );
        assert_eq!(
            ir.image_targets
                .iter()
                .find(|target| target.name == FULL_FRAMEBUFFER_TARGET)
                .expect("SceneColor snapshot target")
                .extent_domain,
            crate::convert::we_ingest::ir::WeIrImageTargetExtentDomain::PhysicalSurface
        );

        let scene =
            crate::convert::we_ingest::lower_ir_to_scene_binary(&ir).expect("lower composelayer");
        assert_eq!(
            scene.render_graphs[0].source_extent_domain,
            crate::engine::scene::SceneRenderSourceExtentDomain::OwnerAuthored
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fullscreen_layer_without_effects_records_no_framebuffer_work() {
        let root = std::env::temp_dir().join(format!(
            "tensor-wallpaper-we-utility-layer-test-{}",
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
        assert_eq!(
            ir.objects[0].render_source_extent_domain,
            crate::convert::we_ingest::ir::WeIrRenderSourceExtentDomain::PhysicalSurface
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

    #[test]
    fn fullscreen_effect_targets_follow_the_physical_graph_source() {
        let root = std::env::temp_dir().join(format!(
            "tensor-wallpaper-we-fullscreen-effect-target-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("effects/post")).expect("effect directory");
        fs::create_dir_all(root.join("materials/effects")).expect("material directory");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":7,"image":"models/util/fullscreenlayer.json","effects":[{"file":"effects/post/effect.json","id":8}]}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("effects/post/effect.json"),
            r#"{"fbos":[{"name":"_rt_HalfSource","format":"rgba_backbuffer","scale":2}],"passes":[{"material":"materials/effects/post.json","target":"_rt_HalfSource","bind":[{"index":0,"target":"previous"}]},{"material":"materials/effects/post.json","bind":[{"index":0,"target":"_rt_HalfSource"}]}]}"#,
        )
        .expect("effect");
        fs::write(
            root.join("materials/effects/post.json"),
            r#"{"passes":[{"shader":"passthrough","blending":"normal"}]}"#,
        )
        .expect("effect material");

        let ir = ingest_wallpaper_engine_project(&root).expect("utility IR");
        let target = ir
            .image_targets
            .iter()
            .find(|target| target.name == "_rt_HalfSource")
            .expect("effect FBO target");
        assert_eq!(
            ir.objects[0].render_source_extent_domain,
            crate::convert::we_ingest::ir::WeIrRenderSourceExtentDomain::PhysicalSurface
        );
        assert_eq!(
            target.extent_domain,
            crate::convert::we_ingest::ir::WeIrImageTargetExtentDomain::GraphSource
        );

        let scene =
            crate::convert::we_ingest::lower_ir_to_scene_binary(&ir).expect("lower utility graph");
        assert_eq!(
            scene.render_graphs[0].source_extent_domain,
            crate::engine::scene::SceneRenderSourceExtentDomain::PhysicalSurface
        );
        assert_eq!(
            scene
                .image_targets
                .iter()
                .find(|target| scene.strings[target.name.0 as usize] == "_rt_HalfSource")
                .expect("lowered effect FBO")
                .extent_domain,
            crate::engine::scene::SceneTargetExtentDomain::GraphSource
        );

        let _ = fs::remove_dir_all(root);
    }
}
