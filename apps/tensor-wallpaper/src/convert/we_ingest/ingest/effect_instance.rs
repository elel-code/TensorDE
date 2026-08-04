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
        let layer_name = bound_string(value.get("name")).unwrap_or_default();
        let mut render_instances = Vec::new();
        for effect in authored_effects {
            let Some(effect_file) = bound_string(effect.get("file")) else {
                continue;
            };
            let effect_handle = self.add_effect(&effect_file)?;
            let instance_id = value_u32(effect.get("id")).unwrap_or(effect_handle);
            let effect_name = bound_string(effect.get("name")).unwrap_or_default();
            let script_may_mutate_effect_visibility = self
                .effect_visibility_mutation_policy
                .may_mutate(&layer_name, &effect_name);
            let visible = bound_bool(effect.get("visible")).unwrap_or(true);
            let binding_start = self.object_effects.len() as u32;
            self.object_effects.push(WeIrObjectEffect {
                object,
                effect: effect_handle,
                name: effect_name,
                instance_id,
                visible,
            });
            if !script_may_mutate_effect_visibility && effect_is_literal_disabled(effect) {
                continue;
            }
            render_instances.push(WeObjectEffectInstance {
                binding_start,
                effect: effect_handle,
                value: effect.clone(),
                runtime_visibility: script_may_mutate_effect_visibility
                    || !effect_visibility_is_literal_or_default(effect),
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
