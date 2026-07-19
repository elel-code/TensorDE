//! Object-level visual properties that must be applied after mesh-group composition.

use serde_json::Value;

use crate::engine::scene::abi::{SceneScriptTarget, SceneVec3};

use super::{WeIrBuilder, json_value::value_u32};

impl WeIrBuilder {
    pub(super) fn puppet_group_visual_required(&self, value: &Value) -> bool {
        if value.get("color").is_some() || value.get("alpha").is_some() {
            return true;
        }
        let mut parent_we_id = value_u32(value.get("parent"));
        while let Some(parent_id) = parent_we_id {
            let Some(parent) = self.objects.iter().find(|object| object.we_id == parent_id) else {
                break;
            };
            if parent.color != SceneVec3::ONE
                || (parent.alpha - 1.0).abs() > f32::EPSILON
                || self.script_programs.iter().any(|program| {
                    program.object == parent.handle
                        && matches!(
                            program.target,
                            SceneScriptTarget::Color | SceneScriptTarget::Alpha
                        )
                })
            {
                return true;
            }
            parent_we_id = parent.parent_we_id;
        }
        false
    }
}
