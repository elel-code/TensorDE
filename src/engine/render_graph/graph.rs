use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::pass::RenderPassNode;
use super::resource::{
    RenderGraphBarrier, RenderGraphResourceAccess, RenderGraphResourceUsage,
    RenderGraphResourceUse, render_target_resource_key, texture_binding_resource_key,
    texture_binding_resource_usage,
};
use super::target::RenderTargetRole;
use super::target::RenderTargetSpec;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderGraphActivationPolicy {
    #[default]
    Always,
    AnyEffectVisible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedGraphBoundary {
    pub object_index: Option<usize>,
    pub pass_index: Option<u32>,
    pub feature: String,
    pub expected_subsystem: String,
    pub containment: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraph {
    #[serde(default)]
    pub activation_policy: RenderGraphActivationPolicy,
    pub passes: Vec<RenderPassNode>,
    #[serde(default)]
    pub target_specs: Vec<RenderTargetSpec>,
    pub unsupported: Vec<UnsupportedGraphBoundary>,
}

impl RenderGraph {
    pub fn topology_hash_inputs(&self) -> Vec<String> {
        self.passes
            .iter()
            .map(|pass| {
                format!(
                    "{:?}:{:?}:{:?}:{:?}:{:?}",
                    pass.role,
                    pass.target,
                    pass.target_name,
                    pass.shader,
                    pass.state.pipeline_blend
                )
            })
            .chain(self.target_specs.iter().map(|target| {
                format!(
                    "target:{:?}:{}:{}:{}:{}",
                    target.role,
                    target.name,
                    target.format,
                    target.width_divisor_milli,
                    target.height_divisor_milli
                )
            }))
            .collect()
    }

    pub fn resource_uses(&self) -> Vec<RenderGraphResourceUse> {
        let mut uses = Vec::new();
        for pass in &self.passes {
            uses.extend(pass.bindings.iter().map(|binding| RenderGraphResourceUse {
                pass_id: pass.id,
                resource_key: texture_binding_resource_key(pass.object_index, binding),
                access: RenderGraphResourceAccess::Read,
                usage: texture_binding_resource_usage(binding),
            }));
            uses.push(RenderGraphResourceUse {
                pass_id: pass.id,
                resource_key: render_target_resource_key(pass.target, pass.target_name.as_deref()),
                access: if pass.target == RenderTargetRole::Swapchain {
                    RenderGraphResourceAccess::Write
                } else {
                    RenderGraphResourceAccess::ReadWrite
                },
                usage: if pass.target == RenderTargetRole::Swapchain {
                    RenderGraphResourceUsage::Present
                } else {
                    RenderGraphResourceUsage::AttachmentColorReadWrite
                },
            });
        }
        uses
    }

    pub fn derived_barriers(&self) -> Vec<RenderGraphBarrier> {
        let mut barriers = Vec::new();
        let mut last_use_by_resource =
            BTreeMap::<String, (u32, RenderGraphResourceAccess, RenderGraphResourceUsage)>::new();
        for resource_use in self.resource_uses() {
            if let Some((before_pass_id, previous_access, previous_usage)) = last_use_by_resource
                .get(&resource_use.resource_key)
                .copied()
            {
                if resource_use.pass_id != before_pass_id
                    && resource_use.access.conflicts_after(previous_access)
                {
                    barriers.push(RenderGraphBarrier {
                        resource_key: resource_use.resource_key.clone(),
                        before_pass_id,
                        after_pass_id: resource_use.pass_id,
                        previous_access,
                        next_access: resource_use.access,
                        previous_usage,
                        next_usage: resource_use.usage,
                    });
                }
            }
            last_use_by_resource.insert(
                resource_use.resource_key,
                (
                    resource_use.pass_id,
                    resource_use.access,
                    resource_use.usage,
                ),
            );
        }
        barriers
    }
}
