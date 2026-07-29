use serde_json::Value;

use crate::{Error, Result, ShaderStage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShaderContract {
    push_constant_bytes: Option<u64>,
    descriptor_mode: DescriptorMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DescriptorMode {
    Free,
    Heap,
}

impl ShaderContract {
    pub const fn descriptor_free(push_constant_bytes: u64) -> Self {
        Self {
            push_constant_bytes: Some(push_constant_bytes),
            descriptor_mode: DescriptorMode::Free,
        }
    }

    pub const fn descriptor_heap(push_constant_bytes: u64) -> Self {
        Self {
            push_constant_bytes: Some(push_constant_bytes),
            descriptor_mode: DescriptorMode::Heap,
        }
    }

    pub(crate) const fn requires_descriptor_heap(self) -> bool {
        matches!(self.descriptor_mode, DescriptorMode::Heap)
    }

    pub(crate) fn validate(
        self,
        reflection: &Value,
        entry_point: &str,
        stage: ShaderStage,
    ) -> Result<()> {
        validate_entry_point(reflection, entry_point, stage)?;
        let parameters = reflection
            .get("parameters")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Reflection("missing top-level parameters array".to_owned()))?;

        let push_constants: Vec<_> = parameters
            .iter()
            .filter(|parameter| binding_kind(parameter) == Some("pushConstantBuffer"))
            .collect();
        if matches!(self.descriptor_mode, DescriptorMode::Free) {
            for parameter in parameters {
                let kind = binding_kind(parameter).ok_or_else(|| {
                    Error::Reflection("parameter is missing its binding kind".to_owned())
                })?;
                if kind != "pushConstantBuffer" {
                    return Err(Error::Reflection(format!(
                        "descriptor-free shader exposes binding kind `{kind}`"
                    )));
                }
            }
        }

        if let Some(expected) = self.push_constant_bytes {
            let [parameter] = push_constants.as_slice() else {
                return Err(Error::Reflection(format!(
                    "expected one push-constant block, found {}",
                    push_constants.len()
                )));
            };
            let found = parameter
                .pointer("/type/elementVarLayout/binding/size")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    Error::Reflection("push-constant block has no reflected byte size".to_owned())
                })?;
            if found != expected {
                return Err(Error::Reflection(format!(
                    "expected {expected} push-constant bytes, found {found}"
                )));
            }
        }
        Ok(())
    }
}

fn binding_kind(parameter: &Value) -> Option<&str> {
    parameter.pointer("/binding/kind").and_then(Value::as_str)
}

fn validate_entry_point(
    reflection: &Value,
    expected_name: &str,
    expected_stage: ShaderStage,
) -> Result<()> {
    let entries = reflection
        .get("entryPoints")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Reflection("missing entryPoints array".to_owned()))?;
    let [entry] = entries.as_slice() else {
        return Err(Error::Reflection(format!(
            "expected one entry point, found {}",
            entries.len()
        )));
    };
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
    let stage = entry.get("stage").and_then(Value::as_str).unwrap_or("");
    if name != expected_name || stage != expected_stage.slang_name() {
        return Err(Error::Reflection(format!(
            "expected {expected_stage} entry `{expected_name}`, found {stage} entry `{name}`"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn descriptor_free_contract_accepts_exact_push_data() {
        let reflection = json!({
            "parameters": [{
                "binding": { "kind": "pushConstantBuffer" },
                "type": { "elementVarLayout": { "binding": { "size": 64 } } }
            }],
            "entryPoints": [{ "name": "mainVertex", "stage": "vertex" }]
        });
        ShaderContract::descriptor_free(64)
            .validate(&reflection, "mainVertex", ShaderStage::Vertex)
            .unwrap();
    }

    #[test]
    fn descriptor_free_contract_rejects_resource_bindings() {
        let reflection = json!({
            "parameters": [{ "binding": { "kind": "descriptorTableSlot" } }],
            "entryPoints": [{ "name": "mainFragment", "stage": "fragment" }]
        });
        assert!(
            ShaderContract {
                push_constant_bytes: None,
                descriptor_mode: DescriptorMode::Free,
            }
            .validate(&reflection, "mainFragment", ShaderStage::Fragment)
            .is_err()
        );
    }
}
