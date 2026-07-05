use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::graph::RenderGraph;
use super::resource::{render_target_resource_key, texture_binding_resource_key};
use super::target::RenderTargetRole;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphTargetAllocationPlan {
    pub logical_target_count: u32,
    pub physical_target_count: u32,
    pub aliased_target_count: u32,
    pub allocations: Vec<RenderGraphTargetAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphTargetAllocation {
    pub resource_key: String,
    pub role: RenderTargetRole,
    pub name: Option<String>,
    pub first_write_pass_id: u32,
    pub last_use_pass_id: u32,
    pub physical_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetLifetime {
    role: RenderTargetRole,
    name: Option<String>,
    first_write_pass_id: u32,
    last_use_pass_id: u32,
}

impl RenderGraph {
    pub fn target_allocation_plan(&self) -> RenderGraphTargetAllocationPlan {
        let mut lifetimes = self.target_lifetimes();
        lifetimes.sort_by(|left, right| {
            left.first_write_pass_id
                .cmp(&right.first_write_pass_id)
                .then_with(|| left.last_use_pass_id.cmp(&right.last_use_pass_id))
                .then_with(|| left.resource_key.cmp(&right.resource_key))
        });

        let mut slot_last_use_pass_ids = Vec::<u32>::new();
        let mut allocations = Vec::with_capacity(lifetimes.len());
        for lifetime in lifetimes {
            let slot = slot_last_use_pass_ids
                .iter()
                .position(|last_use| *last_use < lifetime.first_write_pass_id)
                .unwrap_or_else(|| {
                    slot_last_use_pass_ids.push(0);
                    slot_last_use_pass_ids.len() - 1
                });
            slot_last_use_pass_ids[slot] = lifetime.last_use_pass_id;
            allocations.push(RenderGraphTargetAllocation {
                resource_key: lifetime.resource_key,
                role: lifetime.role,
                name: lifetime.name,
                first_write_pass_id: lifetime.first_write_pass_id,
                last_use_pass_id: lifetime.last_use_pass_id,
                physical_slot: slot.min(u32::MAX as usize) as u32,
            });
        }

        let logical_target_count = allocations.len().min(u32::MAX as usize) as u32;
        let physical_target_count = slot_last_use_pass_ids.len().min(u32::MAX as usize) as u32;
        RenderGraphTargetAllocationPlan {
            logical_target_count,
            physical_target_count,
            aliased_target_count: logical_target_count.saturating_sub(physical_target_count),
            allocations,
        }
    }

    fn target_lifetimes(&self) -> Vec<RenderGraphTargetLifetime> {
        let mut lifetimes = BTreeMap::<String, TargetLifetime>::new();
        for pass in &self.passes {
            if !render_graph_target_role_is_allocatable(pass.target) {
                continue;
            }
            let resource_key = render_target_resource_key(pass.target, pass.target_name.as_deref());
            lifetimes
                .entry(resource_key)
                .and_modify(|lifetime| {
                    lifetime.first_write_pass_id = lifetime.first_write_pass_id.min(pass.id);
                    lifetime.last_use_pass_id = lifetime.last_use_pass_id.max(pass.id);
                })
                .or_insert_with(|| TargetLifetime {
                    role: pass.target,
                    name: pass.target_name.clone(),
                    first_write_pass_id: pass.id,
                    last_use_pass_id: pass.id,
                });
        }

        for pass in &self.passes {
            for binding in &pass.bindings {
                let resource_key = texture_binding_resource_key(pass.object_index, binding);
                if let Some(lifetime) = lifetimes.get_mut(&resource_key) {
                    lifetime.last_use_pass_id = lifetime.last_use_pass_id.max(pass.id);
                }
            }
        }

        lifetimes
            .into_iter()
            .map(|(resource_key, lifetime)| RenderGraphTargetLifetime {
                resource_key,
                role: lifetime.role,
                name: lifetime.name,
                first_write_pass_id: lifetime.first_write_pass_id,
                last_use_pass_id: lifetime.last_use_pass_id,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderGraphTargetLifetime {
    resource_key: String,
    role: RenderTargetRole,
    name: Option<String>,
    first_write_pass_id: u32,
    last_use_pass_id: u32,
}

fn render_graph_target_role_is_allocatable(role: RenderTargetRole) -> bool {
    matches!(
        role,
        RenderTargetRole::ImageLocalMain
            | RenderTargetRole::ImageLocalSub
            | RenderTargetRole::NamedFbo
            | RenderTargetRole::FirstClassEffectTarget
            | RenderTargetRole::Temporary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render_graph::{
        DepthTestMode, PassState, PipelineBlendMode, RenderPassNode, RenderPassRole,
        TextureBindingRole,
    };

    fn pass(
        id: u32,
        target: RenderTargetRole,
        target_name: Option<&str>,
        bindings: Vec<TextureBindingRole>,
    ) -> RenderPassNode {
        RenderPassNode {
            id,
            role: RenderPassRole::EffectMaterial,
            object_index: Some(7),
            pass_index: id,
            shader: None,
            target,
            target_name: target_name.map(str::to_owned),
            bindings,
            state: PassState {
                pipeline_blend: PipelineBlendMode::Normal,
                scene_blend: crate::core::SceneBlendMode::Normal,
                shader_blend: None,
                depth_test: DepthTestMode::Disabled,
                depth_write: false,
                cull_mode: crate::engine::render_graph::CullMode::None,
            },
        }
    }

    #[test]
    fn target_allocation_reuses_non_overlapping_targets() {
        let graph = RenderGraph {
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(
                    1,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![TextureBindingRole::GraphTarget {
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("a".to_owned()),
                    }],
                ),
                pass(2, RenderTargetRole::ImageLocalMain, Some("b"), vec![]),
                pass(
                    3,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![TextureBindingRole::GraphTarget {
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("b".to_owned()),
                    }],
                ),
            ],
            unsupported: Vec::new(),
        };

        let plan = graph.target_allocation_plan();

        assert_eq!(plan.logical_target_count, 2);
        assert_eq!(plan.physical_target_count, 1);
        assert_eq!(plan.aliased_target_count, 1);
        assert_eq!(plan.allocations[0].physical_slot, 0);
        assert_eq!(plan.allocations[1].physical_slot, 0);
    }

    #[test]
    fn target_allocation_keeps_overlapping_target_lifetimes_separate() {
        let graph = RenderGraph {
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(1, RenderTargetRole::ImageLocalMain, Some("b"), vec![]),
                pass(
                    2,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![
                        TextureBindingRole::GraphTarget {
                            role: RenderTargetRole::ImageLocalMain,
                            name: Some("a".to_owned()),
                        },
                        TextureBindingRole::GraphTarget {
                            role: RenderTargetRole::ImageLocalMain,
                            name: Some("b".to_owned()),
                        },
                    ],
                ),
            ],
            unsupported: Vec::new(),
        };

        let plan = graph.target_allocation_plan();

        assert_eq!(plan.logical_target_count, 2);
        assert_eq!(plan.physical_target_count, 2);
        assert_eq!(plan.aliased_target_count, 0);
    }
}
