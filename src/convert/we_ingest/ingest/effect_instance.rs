//! Object effect instance selection and static visibility proof on the convert cold path.

use serde_json::Value;

use crate::convert::we_ingest::ir::WeIrObjectEffect;

use super::json_value::{bound_bool, bound_string, value_u32};
use super::{WeIngestError, WeIrBuilder};

#[derive(Debug, Clone)]
pub(super) struct WeObjectEffectInstance {
    pub(super) binding_start: u32,
    pub(super) effect: u32,
    pub(super) value: Value,
    pub(super) runtime_visibility: bool,
}

impl WeIrBuilder {
    pub(super) fn add_object_effect_instances(
        &mut self,
        object: u32,
        value: &Value,
    ) -> Result<Vec<WeObjectEffectInstance>, WeIngestError> {
        let authored_effects = value
            .get("effects")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let prune_literal_disabled_effects = !self.scene_scripts_may_mutate_effect_visibility
            && authored_effects
                .iter()
                .all(effect_visibility_is_literal_or_default)
            && authored_effects.iter().any(effect_is_literal_disabled);
        let mut render_instances = Vec::new();
        for effect in authored_effects {
            let Some(effect_file) = bound_string(effect.get("file")) else {
                continue;
            };
            let effect_handle = self.add_effect(&effect_file)?;
            let instance_id = value_u32(effect.get("id")).unwrap_or(effect_handle);
            let visible = bound_bool(effect.get("visible")).unwrap_or(true);
            let binding_start = self.object_effects.len() as u32;
            self.object_effects.push(WeIrObjectEffect {
                object,
                effect: effect_handle,
                name: bound_string(effect.get("name")).unwrap_or_default(),
                instance_id,
                visible,
            });
            if prune_literal_disabled_effects && effect_is_literal_disabled(effect) {
                continue;
            }
            render_instances.push(WeObjectEffectInstance {
                binding_start,
                effect: effect_handle,
                value: effect.clone(),
                runtime_visibility: !prune_literal_disabled_effects,
            });
        }
        Ok(render_instances)
    }
}

fn effect_visibility_is_literal_or_default(effect: &Value) -> bool {
    effect.get("visible").is_none_or(Value::is_boolean)
}

fn effect_is_literal_disabled(effect: &Value) -> bool {
    effect.get("visible").and_then(Value::as_bool) == Some(false)
}
