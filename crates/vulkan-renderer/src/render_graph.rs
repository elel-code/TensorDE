use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use vulkanalia::vk;

use crate::command::BufferState;

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
pub enum RenderGraphImageState {
    Undefined,
    ColorAttachmentWrite,
    AttachmentWrite,
    ColorAttachmentReadWrite,
    RenderingLocalRead,
    FragmentSampledRead,
    FragmentSampledReadGeneral,
    ComputeSampledRead,
    CopySource,
    CopyDestination,
    ClearDestination,
    BlitSource,
    BlitDestination,
    TransferSource,
    TransferDestination,
    StorageReadWrite,
    Present,
}

impl RenderGraphImageState {
    fn synchronization(self) -> (vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout) {
        match self {
            Self::Undefined => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::UNDEFINED,
            ),
            Self::ColorAttachmentWrite => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            Self::AttachmentWrite => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::ImageLayout::ATTACHMENT_OPTIMAL,
            ),
            Self::ColorAttachmentReadWrite => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::ImageLayout::ATTACHMENT_OPTIMAL,
            ),
            Self::RenderingLocalRead => (
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags2::INPUT_ATTACHMENT_READ,
                vk::ImageLayout::RENDERING_LOCAL_READ,
            ),
            Self::FragmentSampledRead => (
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ),
            Self::FragmentSampledReadGeneral => (
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::GENERAL,
            ),
            Self::ComputeSampledRead => (
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ),
            Self::CopySource => (
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_READ,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ),
            Self::CopyDestination => (
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ),
            Self::ClearDestination => (
                vk::PipelineStageFlags2::CLEAR,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ),
            Self::BlitSource => (
                vk::PipelineStageFlags2::BLIT,
                vk::AccessFlags2::TRANSFER_READ,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ),
            Self::BlitDestination => (
                vk::PipelineStageFlags2::BLIT,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ),
            Self::TransferSource => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_READ,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ),
            Self::TransferDestination => (
                vk::PipelineStageFlags2::ALL_TRANSFER,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ),
            Self::StorageReadWrite => (
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
            ),
            Self::Present => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::PRESENT_SRC_KHR,
            ),
        }
    }
}

/// The external owner-visible layout of an imported or exported image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForeignImageState {
    Undefined,
    General,
}

impl ForeignImageState {
    fn synchronization(self) -> (vk::PipelineStageFlags2, vk::AccessFlags2, vk::ImageLayout) {
        match self {
            Self::Undefined => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::UNDEFINED,
            ),
            Self::General => (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::GENERAL,
            ),
        }
    }
}

/// A semantic synchronization state used by a [`RenderGraph`].
///
/// Product code selects one of the explicit resource-use states instead of
/// assembling Vulkan stage, access, and layout flags. The renderer owns the
/// exact synchronization2 lowering for every variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceState {
    Buffer {
        state: BufferState,
        queue_family: u32,
    },
    Image {
        state: RenderGraphImageState,
        queue_family: u32,
    },
    ForeignImage {
        state: ForeignImageState,
    },
}

impl ResourceState {
    pub const fn buffer(state: BufferState, queue_family: u32) -> Self {
        Self::Buffer {
            state,
            queue_family,
        }
    }

    pub const fn image(state: RenderGraphImageState, queue_family: u32) -> Self {
        Self::Image {
            state,
            queue_family,
        }
    }

    pub const fn buffer_copy_destination(queue_family: u32) -> Self {
        Self::buffer(BufferState::TransferDestination, queue_family)
    }

    pub const fn vertex_buffer(queue_family: u32) -> Self {
        Self::buffer(BufferState::VertexRead, queue_family)
    }

    pub const fn color_attachment_write(queue_family: u32) -> Self {
        Self::image(RenderGraphImageState::AttachmentWrite, queue_family)
    }

    pub const fn present(queue_family: u32) -> Self {
        Self::image(RenderGraphImageState::Present, queue_family)
    }

    /// State of an externally owned image before an acquire or after a
    /// release. Fresh imports use [`ForeignImageState::Undefined`];
    /// initialized dma-bufs commonly preserve [`ForeignImageState::General`]
    /// between submissions.
    pub const fn foreign_image(state: ForeignImageState) -> Self {
        Self::ForeignImage { state }
    }

    pub const fn resource_kind(self) -> ResourceKind {
        match self {
            Self::Buffer { .. } => ResourceKind::Buffer,
            Self::Image { .. } | Self::ForeignImage { .. } => ResourceKind::Image,
        }
    }

    pub const fn queue_family(self) -> u32 {
        match self {
            Self::Buffer { queue_family, .. } | Self::Image { queue_family, .. } => queue_family,
            Self::ForeignImage { .. } => vk::QUEUE_FAMILY_FOREIGN_EXT,
        }
    }

    pub const fn image_state(self) -> Option<RenderGraphImageState> {
        match self {
            Self::Image { state, .. } => Some(state),
            Self::Buffer { .. } | Self::ForeignImage { .. } => None,
        }
    }

    pub const fn foreign_image_state(self) -> Option<ForeignImageState> {
        match self {
            Self::ForeignImage { state } => Some(state),
            Self::Buffer { .. } | Self::Image { .. } => None,
        }
    }

    pub const fn buffer_state(self) -> Option<BufferState> {
        match self {
            Self::Buffer { state, .. } => Some(state),
            Self::Image { .. } | Self::ForeignImage { .. } => None,
        }
    }

    pub(crate) fn synchronization(
        self,
    ) -> (
        vk::PipelineStageFlags2,
        vk::AccessFlags2,
        vk::ImageLayout,
        u32,
    ) {
        match self {
            Self::Buffer {
                state,
                queue_family,
            } => {
                let (stages, access, _) = state.synchronization();
                (stages, access, vk::ImageLayout::UNDEFINED, queue_family)
            }
            Self::Image {
                state,
                queue_family,
            } => {
                let (stages, access, layout) = state.synchronization();
                (stages, access, layout, queue_family)
            }
            Self::ForeignImage { state } => {
                let (stages, access, layout) = state.synchronization();
                (stages, access, layout, vk::QUEUE_FAMILY_FOREIGN_EXT)
            }
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
    /// `None` denotes an imported/created resource state before the first pass.
    pub before: Option<PassId>,
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
    initial_states: BTreeMap<ResourceId, (ResourceKind, ResourceState)>,
}

impl RenderGraph {
    pub fn add_pass(&mut self, pass: RenderPass) {
        self.passes.push(pass);
    }

    pub fn passes(&self) -> &[RenderPass] {
        &self.passes
    }

    /// Declares the state and queue ownership in which an imported or newly
    /// created resource enters this graph. Replaces a prior declaration and
    /// returns it.
    pub fn set_initial_state(
        &mut self,
        resource: ResourceId,
        kind: ResourceKind,
        state: ResourceState,
    ) -> Option<(ResourceKind, ResourceState)> {
        self.initial_states.insert(resource, (kind, state))
    }

    pub fn compile(&self) -> std::result::Result<CompiledGraph, RenderGraphError> {
        for (resource, (kind, state)) in &self.initial_states {
            if *kind != state.resource_kind() {
                return Err(RenderGraphError::ResourceStateKindMismatch {
                    resource: *resource,
                    kind: *kind,
                    state_kind: state.resource_kind(),
                });
            }
        }
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
        let mut previous = self
            .initial_states
            .iter()
            .map(|(resource, (kind, state))| {
                (
                    *resource,
                    (
                        None,
                        ResourceUse {
                            resource: *resource,
                            kind: *kind,
                            access: AccessKind::Read,
                            state: *state,
                        },
                    ),
                )
            })
            .collect::<BTreeMap<ResourceId, (Option<usize>, ResourceUse)>>();
        for (after, pass) in self.passes.iter().enumerate() {
            let mut in_pass = BTreeSet::new();
            for usage in &pass.resources {
                if usage.kind != usage.state.resource_kind() {
                    return Err(RenderGraphError::ResourceStateKindMismatch {
                        resource: usage.resource,
                        kind: usage.kind,
                        state_kind: usage.state.resource_kind(),
                    });
                }
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
                        if let Some(before) = before {
                            edges.insert((before, after));
                        }
                        barriers.push(Barrier {
                            resource: usage.resource,
                            kind: usage.kind,
                            before: before.map(|before| self.passes[before].id),
                            after: pass.id,
                            source: prior.state,
                            destination: usage.state,
                        });
                    }
                }
                previous.insert(usage.resource, (Some(after), *usage));
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
    UnknownDependency {
        pass: PassId,
        dependency: PassId,
    },
    DuplicateResourceUse {
        pass: PassId,
        resource: ResourceId,
    },
    ResourceKindChanged(ResourceId),
    ResourceStateKindMismatch {
        resource: ResourceId,
        kind: ResourceKind,
        state_kind: ResourceKind,
    },
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

    fn image_use(resource: u64, access: AccessKind, state: RenderGraphImageState) -> ResourceUse {
        ResourceUse {
            resource: ResourceId(resource),
            kind: ResourceKind::Image,
            access,
            state: ResourceState::image(state, 0),
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
                RenderGraphImageState::ColorAttachmentWrite,
            )],
        });
        graph.add_pass(RenderPass {
            id: PassId(20),
            label: "composite".into(),
            depends_on: vec![],
            resources: vec![image_use(
                7,
                AccessKind::Read,
                RenderGraphImageState::FragmentSampledRead,
            )],
        });
        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.ordered_passes, vec![PassId(10), PassId(20)]);
        assert_eq!(compiled.barriers.len(), 1);
        assert_eq!(compiled.barriers[0].resource, ResourceId(7));
        assert_eq!(compiled.barriers[0].before, Some(PassId(10)));
    }

    #[test]
    fn initial_image_state_transitions_before_the_first_pass() {
        let resource = ResourceId(3);
        let mut graph = RenderGraph::default();
        graph.set_initial_state(
            resource,
            ResourceKind::Image,
            ResourceState::image(RenderGraphImageState::Undefined, 4),
        );
        graph.add_pass(RenderPass {
            id: PassId(1),
            label: "sample".into(),
            depends_on: vec![],
            resources: vec![ResourceUse {
                resource,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, 4),
            }],
        });
        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.barriers.len(), 1);
        assert_eq!(compiled.barriers[0].before, None);
        assert_eq!(compiled.barriers[0].after, PassId(1));
        assert_eq!(
            compiled.barriers[0].source.image_state(),
            Some(RenderGraphImageState::Undefined)
        );
    }

    #[test]
    fn semantic_states_preserve_exact_vulkan_contracts() {
        let queue = 3;
        assert_eq!(
            ResourceState::buffer_copy_destination(queue),
            ResourceState::buffer(BufferState::TransferDestination, queue)
        );
        assert_eq!(
            ResourceState::vertex_buffer(queue),
            ResourceState::buffer(BufferState::VertexRead, queue)
        );
        assert_eq!(
            ResourceState::color_attachment_write(queue).image_state(),
            Some(RenderGraphImageState::AttachmentWrite)
        );
        assert_eq!(
            ResourceState::present(queue).image_state(),
            Some(RenderGraphImageState::Present)
        );
    }

    #[test]
    fn typed_image_states_preserve_transfer_and_sampling_scopes() {
        assert_eq!(
            RenderGraphImageState::ClearDestination.synchronization(),
            (
                vk::PipelineStageFlags2::CLEAR,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            )
        );
        assert_eq!(
            RenderGraphImageState::BlitSource.synchronization(),
            (
                vk::PipelineStageFlags2::BLIT,
                vk::AccessFlags2::TRANSFER_READ,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            )
        );
        assert_eq!(
            RenderGraphImageState::BlitDestination.synchronization(),
            (
                vk::PipelineStageFlags2::BLIT,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            )
        );
        assert_eq!(
            RenderGraphImageState::FragmentSampledRead.synchronization(),
            (
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )
        );
    }

    #[test]
    fn state_kind_mismatch_is_rejected_before_barrier_lowering() {
        let mut graph = RenderGraph::default();
        graph.add_pass(RenderPass {
            id: PassId(1),
            label: "invalid".into(),
            depends_on: vec![],
            resources: vec![ResourceUse {
                resource: ResourceId(1),
                kind: ResourceKind::Buffer,
                access: AccessKind::Read,
                state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, 0),
            }],
        });
        assert_eq!(
            graph.compile(),
            Err(RenderGraphError::ResourceStateKindMismatch {
                resource: ResourceId(1),
                kind: ResourceKind::Buffer,
                state_kind: ResourceKind::Image,
            })
        );
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
