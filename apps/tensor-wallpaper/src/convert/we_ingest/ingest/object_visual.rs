//! Object-level visual properties that must be applied after mesh-group composition.

use serde_json::Value;

use crate::engine::scene::abi::SceneScriptTarget;

use super::WeIrBuilder;

impl WeIrBuilder {
    pub(super) fn puppet_group_visual_required(&self, value: &Value) -> bool {
        if value.get("color").is_some() || value.get("alpha").is_some() {
            return true;
        }
        let object = self.objects.len() as u32;
        self.script_programs.iter().any(|program| {
            program.object == object
                && matches!(
                    program.target,
                    SceneScriptTarget::Color | SceneScriptTarget::Alpha
                )
        })
    }
}
