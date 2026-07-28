use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use vulkanalia::vk;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PassId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Buffer,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessKind {
    Read,
    Write,
    ReadWrite,
}

impl AccessKind {
    const fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceState {
    pub stages: vk::PipelineStageFlags2,
    pub access: vk::AccessFlags2,
    pub layout: vk::ImageLayout,
    pub queue_family: u32,
}

impl ResourceState {
    pub const fn buffer(
        stages: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
        queue_family: u32,
    ) -> Self {
        Self {
            stages,
            access,
            layout: vk::ImageLayout::UNDEFINED,
            queue_family,
        }
    }

    pub const fn image(
        stages: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
        layout: vk::ImageLayout,
        queue_family: u32,
    ) -> Self {
        Self {
            stages,
            access,
            layout,
            queue_family,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUse {
    pub resource: ResourceId,
    pub kind: ResourceKind,
    pub access: AccessKind,
    pub state: ResourceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPass {
    pub id: PassId,
    pub label: String,
    pub depends_on: Vec<PassId>,
    pub resources: Vec<ResourceUse>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Barrier {
    pub resource: ResourceId,
    pub kind: ResourceKind,
    pub before: PassId,
    pub after: PassId,
    pub source: ResourceState,
    pub destination: ResourceState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompiledGraph {
    pub ordered_passes: Vec<PassId>,
    pub barriers: Vec<Barrier>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RenderGraph {
    passes: Vec<RenderPass>,
}

impl RenderGraph {
    pub fn add_pass(&mut self, pass: RenderPass) {
        self.passes.push(pass);
    }

    pub fn passes(&self) -> &[RenderPass] {
        &self.passes
    }

    pub fn compile(&self) -> std::result::Result<CompiledGraph, RenderGraphError> {
        let mut index_by_id = BTreeMap::new();
        for (index, pass) in self.passes.iter().enumerate() {
            if index_by_id.insert(pass.id, index).is_some() {
                return Err(RenderGraphError::DuplicatePass(pass.id));
            }
        }

        let mut edges = BTreeSet::<(usize, usize)>::new();
        for (after, pass) in self.passes.iter().enumerate() {
            for dependency in &pass.depends_on {
                let before = index_by_id.get(dependency).copied().ok_or(
                    RenderGraphError::UnknownDependency {
                        pass: pass.id,
                        dependency: *dependency,
                    },
                )?;
                edges.insert((before, after));
            }
        }

        let mut barriers = Vec::new();
        let mut previous = BTreeMap::<ResourceId, (usize, ResourceUse)>::new();
        for (after, pass) in self.passes.iter().enumerate() {
            let mut in_pass = BTreeSet::new();
            for usage in &pass.resources {
                if !in_pass.insert(usage.resource) {
                    return Err(RenderGraphError::DuplicateResourceUse {
                        pass: pass.id,
                        resource: usage.resource,
                    });
                }
                if let Some((before, prior)) = previous.get(&usage.resource).copied() {
                    if prior.kind != usage.kind {
                        return Err(RenderGraphError::ResourceKindChanged(usage.resource));
                    }
                    let state_changed = prior.state != usage.state;
                    if prior.access.writes() || usage.access.writes() || state_changed {
                        edges.insert((before, after));
                        barriers.push(Barrier {
                            resource: usage.resource,
                            kind: usage.kind,
                            before: self.passes[before].id,
                            after: pass.id,
                            source: prior.state,
                            destination: usage.state,
                        });
                    }
                }
                previous.insert(usage.resource, (after, *usage));
            }
        }

        let ordered_indices = topological_order(self.passes.len(), &edges)?;
        Ok(CompiledGraph {
            ordered_passes: ordered_indices
                .into_iter()
                .map(|index| self.passes[index].id)
                .collect(),
            barriers,
        })
    }
}

fn topological_order(
    pass_count: usize,
    edges: &BTreeSet<(usize, usize)>,
) -> std::result::Result<Vec<usize>, RenderGraphError> {
    let mut successors = vec![Vec::new(); pass_count];
    let mut indegrees = vec![0usize; pass_count];
    for &(before, after) in edges {
        if before == after {
            return Err(RenderGraphError::Cycle);
        }
        successors[before].push(after);
        indegrees[after] += 1;
    }
    let mut ready = (0..pass_count)
        .filter(|index| indegrees[*index] == 0)
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(pass_count);
    while let Some(index) = ready.pop_first() {
        ordered.push(index);
        for successor in &successors[index] {
            indegrees[*successor] -= 1;
            if indegrees[*successor] == 0 {
                ready.insert(*successor);
            }
        }
    }
    if ordered.len() == pass_count {
        Ok(ordered)
    } else {
        Err(RenderGraphError::Cycle)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderGraphError {
    DuplicatePass(PassId),
    UnknownDependency { pass: PassId, dependency: PassId },
    DuplicateResourceUse { pass: PassId, resource: ResourceId },
    ResourceKindChanged(ResourceId),
    Cycle,
}

impl fmt::Display for RenderGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RenderGraphError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_use(resource: u64, access: AccessKind, layout: vk::ImageLayout) -> ResourceUse {
        ResourceUse {
            resource: ResourceId(resource),
            kind: ResourceKind::Image,
            access,
            state: ResourceState::image(
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                layout,
                0,
            ),
        }
    }

    #[test]
    fn write_then_sample_derives_dependency_and_layout_barrier() {
        let mut graph = RenderGraph::default();
        graph.add_pass(RenderPass {
            id: PassId(10),
            label: "offscreen".into(),
            depends_on: vec![],
            resources: vec![image_use(
                7,
                AccessKind::Write,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            )],
        });
        graph.add_pass(RenderPass {
            id: PassId(20),
            label: "composite".into(),
            depends_on: vec![],
            resources: vec![image_use(
                7,
                AccessKind::Read,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )],
        });
        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.ordered_passes, vec![PassId(10), PassId(20)]);
        assert_eq!(compiled.barriers.len(), 1);
        assert_eq!(compiled.barriers[0].resource, ResourceId(7));
    }

    #[test]
    fn explicit_cycle_is_rejected() {
        let mut graph = RenderGraph::default();
        graph.add_pass(RenderPass {
            id: PassId(1),
            label: "a".into(),
            depends_on: vec![PassId(2)],
            resources: vec![],
        });
        graph.add_pass(RenderPass {
            id: PassId(2),
            label: "b".into(),
            depends_on: vec![PassId(1)],
            resources: vec![],
        });
        assert_eq!(graph.compile(), Err(RenderGraphError::Cycle));
    }
}
