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
    pub extent: Option<[u32; 2]>,
    pub format: Option<String>,
    pub first_write_pass_id: u32,
    pub last_use_pass_id: u32,
    pub physical_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetLifetime {
    role: RenderTargetRole,
    name: Option<String>,
    extent: Option<[u32; 2]>,
    format: Option<String>,
    has_unknown_extent: bool,
    first_write_pass_id: u32,
    last_use_pass_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhysicalTargetSlot {
    extent: Option<[u32; 2]>,
    format: Option<String>,
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

        let mut physical_slots = Vec::<PhysicalTargetSlot>::new();
        let mut allocations = Vec::with_capacity(lifetimes.len());
        for lifetime in lifetimes {
            let slot = physical_slots
                .iter()
                .position(|slot| {
                    slot.last_use_pass_id < lifetime.first_write_pass_id
                        && lifetime.extent.is_some()
                        && slot.extent == lifetime.extent
                        && slot.format == lifetime.format
                })
                .unwrap_or_else(|| {
                    physical_slots.push(PhysicalTargetSlot {
                        extent: lifetime.extent,
                        format: lifetime.format.clone(),
                        last_use_pass_id: 0,
                    });
                    physical_slots.len() - 1
                });
            physical_slots[slot].last_use_pass_id = lifetime.last_use_pass_id;
            allocations.push(RenderGraphTargetAllocation {
                resource_key: lifetime.resource_key,
                role: lifetime.role,
                name: lifetime.name,
                extent: lifetime.extent,
                format: lifetime.format,
                first_write_pass_id: lifetime.first_write_pass_id,
                last_use_pass_id: lifetime.last_use_pass_id,
                physical_slot: slot.min(u32::MAX as usize) as u32,
            });
        }

        let logical_target_count = allocations.len().min(u32::MAX as usize) as u32;
        let physical_target_count = physical_slots.len().min(u32::MAX as usize) as u32;
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
                    target_lifetime_record_extent(lifetime, pass.target_extent);
                    target_lifetime_record_format(lifetime, pass.target_format.as_deref());
                })
                .or_insert_with(|| TargetLifetime {
                    role: pass.target,
                    name: pass.target_name.clone(),
                    extent: pass.target_extent,
                    format: pass.target_format.clone(),
                    has_unknown_extent: pass.target_extent.is_none(),
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
                extent: lifetime.extent,
                format: lifetime.format,
                first_write_pass_id: lifetime.first_write_pass_id,
                last_use_pass_id: lifetime.last_use_pass_id,
            })
            .collect()
    }
}

fn target_lifetime_record_extent(lifetime: &mut TargetLifetime, extent: Option<[u32; 2]>) {
    if lifetime.has_unknown_extent {
        return;
    }
    let Some(extent) = extent else {
        lifetime.extent = None;
        lifetime.has_unknown_extent = true;
        return;
    };
    match lifetime.extent {
        Some(existing) if existing != extent => {
            lifetime.extent = None;
            lifetime.has_unknown_extent = true;
        }
        Some(_) => {}
        None => lifetime.extent = Some(extent),
    }
}

fn target_lifetime_record_format(lifetime: &mut TargetLifetime, format: Option<&str>) {
    let format = format.map(str::to_owned);
    match (&lifetime.format, format) {
        (Some(existing), Some(next)) if existing != &next => lifetime.format = None,
        (None, Some(next)) => lifetime.format = Some(next),
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderGraphTargetLifetime {
    resource_key: String,
    role: RenderTargetRole,
    name: Option<String>,
    extent: Option<[u32; 2]>,
    format: Option<String>,
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
            target_extent: Some([512, 256]),
            target_format: None,
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

    #[test]
    fn target_allocation_keeps_different_extents_separate() {
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
                RenderPassNode {
                    target_extent: Some([1024, 256]),
                    ..pass(2, RenderTargetRole::ImageLocalMain, Some("b"), vec![])
                },
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
        assert_eq!(plan.physical_target_count, 2);
        assert_eq!(plan.aliased_target_count, 0);
    }

    #[test]
    fn target_allocation_keeps_different_formats_separate() {
        let graph = RenderGraph {
            passes: vec![
                RenderPassNode {
                    target_format: Some("rgba8".to_owned()),
                    ..pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![])
                },
                pass(
                    1,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![TextureBindingRole::GraphTarget {
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("a".to_owned()),
                    }],
                ),
                RenderPassNode {
                    target_format: Some("rgba16f".to_owned()),
                    ..pass(2, RenderTargetRole::ImageLocalMain, Some("b"), vec![])
                },
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
        assert_eq!(plan.physical_target_count, 2);
        assert_eq!(plan.aliased_target_count, 0);
    }

    #[test]
    fn target_allocation_keeps_unknown_extents_separate() {
        let graph = RenderGraph {
            passes: vec![
                RenderPassNode {
                    target_extent: None,
                    ..pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![])
                },
                pass(
                    1,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![TextureBindingRole::GraphTarget {
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("a".to_owned()),
                    }],
                ),
                RenderPassNode {
                    target_extent: None,
                    ..pass(2, RenderTargetRole::ImageLocalMain, Some("b"), vec![])
                },
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
        assert_eq!(plan.physical_target_count, 2);
        assert_eq!(plan.aliased_target_count, 0);
    }
}
