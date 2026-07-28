//! Lower authored audio and pointer bindings into typed static records.

use crate::engine::scene::{
    SceneCameraParallaxRecord, SceneObjectHandle, SceneObjectParallaxDepthRecord,
    SceneScriptProgramRecord,
};

use super::super::ir::WeSceneIr;
use super::StringInterner;

pub(super) struct LoweredSceneEventBindings {
    pub(super) scripts: Vec<SceneScriptProgramRecord>,
    pub(super) camera_parallax: SceneCameraParallaxRecord,
    pub(super) object_parallax_depths: Vec<SceneObjectParallaxDepthRecord>,
}

pub(super) fn lower_event_bindings(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> LoweredSceneEventBindings {
    let object_parallax_depths = ir
        .objects
        .iter()
        .filter(|object| object.parallax_depth != [0.0; 2])
        .map(|object| SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(object.handle),
            depth: object.parallax_depth,
        })
        .collect();
    let scripts = ir
        .script_programs
        .iter()
        .map(|program| SceneScriptProgramRecord {
            object: SceneObjectHandle(program.object),
            target: program.target,
            source: strings.id(&program.source),
            properties_json: strings.id(&program.properties_json),
            initial_text: strings.optional_id(&program.initial_text),
            subscriptions: program.subscriptions,
            initial_numeric: program.initial_numeric,
        })
        .collect();
    LoweredSceneEventBindings {
        scripts,
        camera_parallax: SceneCameraParallaxRecord {
            enabled: ir.scene.camera_parallax_enabled,
            amount: ir.scene.camera_parallax_amount,
            delay: ir.scene.camera_parallax_delay,
            mouse_influence: ir.scene.camera_parallax_mouse_influence,
        },
        object_parallax_depths,
    }
}
