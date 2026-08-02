//! Shared direct-surface and offscreen-composition policy.
//!
//! Products describe the complete graph facts that affect presentation. The
//! renderer has no architectural primary path: products explicitly select a
//! direct surface, an offscreen target, or automatic fact-based resolution.

use std::fmt;

use crate::pipeline::{format_has_depth, format_has_stencil};
use crate::{
    Backend, DescriptorHeap, DescriptorHeapDescriptor, DescriptorHeapKind, Error, Extent2D, Image,
    ImageDescriptor, ImageView, ImageViewDescriptor, MemoryAllocator, MemoryLocation, Result,
    SampledImageBinding, SampledTextureHeapIndices, SamplerBinding, SamplerDescriptor,
    TextureFormat, TextureUsages,
};

/// Caller preference for the color target that precedes presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameTargetPreference {
    /// Use the swapchain only when every direct-alias condition is proven.
    Automatic,
    /// Require direct swapchain rendering and fail if any condition is unmet.
    DirectSurface,
    /// Always retain a distinct offscreen color target and terminal composite.
    Offscreen,
}

/// Point at which the swapchain image enters the frame transaction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfaceAcquireStrategy {
    /// Acquire before recording/submitting the frame's graphics work.
    #[default]
    BeforeFrame,
    /// Submit swapchain-independent offscreen work first, then acquire and
    /// submit the terminal composite separately.
    AfterOffscreenSubmit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalSampling {
    Nearest,
    #[default]
    Linear,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalAlphaMode {
    #[default]
    Preserve,
    Opaque,
}

/// Stable terminal-composite behavior selected by the product.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalCompositeDescriptor {
    pub sampling: TerminalSampling,
    pub alpha: TerminalAlphaMode,
}

/// Complete graph facts needed to decide whether swapchain aliasing is legal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationRequirements {
    pub surface_extent: Extent2D,
    pub target_extent: Extent2D,
    pub surface_format: TextureFormat,
    pub target_format: TextureFormat,
    pub frame_slots: u32,
    pub physical_pass_count: u32,
    pub sampled_after_write: bool,
    pub has_history: bool,
    pub has_external_consumer: bool,
    pub uses_async_compute: bool,
    pub requires_terminal_transform: bool,
}

/// Product-selected policy. It is separate from graph facts so formal A/B can
/// switch acquire timing without changing authored graph semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationPathDescriptor {
    pub target: FrameTargetPreference,
    pub acquire: SurfaceAcquireStrategy,
    pub terminal: TerminalCompositeDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationTarget {
    DirectSurface,
    Offscreen,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DirectSurfaceBlocker {
    MultiplePhysicalPasses,
    SampledAfterWrite,
    History,
    ExternalConsumer,
    AsyncCompute,
    ExtentMismatch,
    FormatMismatch,
    TerminalTransform,
}

impl fmt::Display for DirectSurfaceBlocker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

/// Validated target/acquire decision. An offscreen plan always has an explicit
/// terminal composite; a direct plan never has one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationPathPlan {
    pub target: PresentationTarget,
    pub surface_extent: Extent2D,
    pub surface_format: TextureFormat,
    pub target_extent: Extent2D,
    pub target_format: TextureFormat,
    pub frame_slots: u32,
    pub acquire: SurfaceAcquireStrategy,
    pub terminal: Option<TerminalCompositeDescriptor>,
    pub direct_surface_blockers: Vec<DirectSurfaceBlocker>,
}

impl PresentationPathPlan {
    pub fn compile(
        descriptor: PresentationPathDescriptor,
        requirements: PresentationRequirements,
    ) -> Result<Self> {
        validate_requirements(requirements)?;
        let blockers = direct_surface_blockers(descriptor, requirements);
        let target = match descriptor.target {
            FrameTargetPreference::Automatic if blockers.is_empty() => {
                PresentationTarget::DirectSurface
            }
            FrameTargetPreference::Automatic | FrameTargetPreference::Offscreen => {
                PresentationTarget::Offscreen
            }
            FrameTargetPreference::DirectSurface if blockers.is_empty() => {
                PresentationTarget::DirectSurface
            }
            FrameTargetPreference::DirectSurface => {
                return Err(Error::Validation(format!(
                    "direct surface target is incompatible with the frame graph: {}",
                    blockers
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        };
        if target == PresentationTarget::DirectSurface
            && descriptor.acquire == SurfaceAcquireStrategy::AfterOffscreenSubmit
        {
            return Err(Error::Validation(
                "late surface acquire requires swapchain-independent offscreen work".into(),
            ));
        }
        Ok(Self {
            target,
            surface_extent: requirements.surface_extent,
            surface_format: requirements.surface_format,
            target_extent: requirements.target_extent,
            target_format: requirements.target_format,
            frame_slots: requirements.frame_slots,
            acquire: descriptor.acquire,
            terminal: (target == PresentationTarget::Offscreen).then_some(descriptor.terminal),
            direct_surface_blockers: blockers,
        })
    }
}

fn validate_requirements(requirements: PresentationRequirements) -> Result<()> {
    if requirements.surface_extent.is_empty() || requirements.target_extent.is_empty() {
        return Err(Error::Validation(
            "presentation extents must be non-zero".into(),
        ));
    }
    if requirements.frame_slots == 0 {
        return Err(Error::Validation(
            "presentation requires at least one in-flight frame slot".into(),
        ));
    }
    if requirements.physical_pass_count == 0 {
        return Err(Error::Validation(
            "presentation requires at least one physical render pass".into(),
        ));
    }
    Ok(())
}

fn direct_surface_blockers(
    descriptor: PresentationPathDescriptor,
    requirements: PresentationRequirements,
) -> Vec<DirectSurfaceBlocker> {
    let mut blockers = Vec::new();
    if requirements.physical_pass_count != 1 {
        blockers.push(DirectSurfaceBlocker::MultiplePhysicalPasses);
    }
    if requirements.sampled_after_write {
        blockers.push(DirectSurfaceBlocker::SampledAfterWrite);
    }
    if requirements.has_history {
        blockers.push(DirectSurfaceBlocker::History);
    }
    if requirements.has_external_consumer {
        blockers.push(DirectSurfaceBlocker::ExternalConsumer);
    }
    if requirements.uses_async_compute {
        blockers.push(DirectSurfaceBlocker::AsyncCompute);
    }
    if requirements.target_extent != requirements.surface_extent {
        blockers.push(DirectSurfaceBlocker::ExtentMismatch);
    }
    if requirements.target_format != requirements.surface_format {
        blockers.push(DirectSurfaceBlocker::FormatMismatch);
    }
    if requirements.requires_terminal_transform
        || descriptor.terminal.alpha != TerminalAlphaMode::Preserve
    {
        blockers.push(DirectSurfaceBlocker::TerminalTransform);
    }
    blockers
}

/// Shared allocation contract for one color image per in-flight frame slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OffscreenColorTargetsDescriptor {
    pub label: Option<String>,
    pub extent: Extent2D,
    pub format: TextureFormat,
    pub frame_slots: u32,
    /// Extra usage beyond the mandatory color-attachment and sampled roles.
    pub additional_usage: TextureUsages,
}

impl OffscreenColorTargetsDescriptor {
    pub fn from_plan(label: Option<String>, plan: &PresentationPathPlan) -> Result<Self> {
        if plan.target != PresentationTarget::Offscreen {
            return Err(Error::Validation(
                "offscreen targets require an offscreen presentation plan".into(),
            ));
        }
        Ok(Self {
            label,
            extent: plan.target_extent,
            format: plan.target_format,
            frame_slots: plan.frame_slots,
            additional_usage: TextureUsages::empty(),
        })
    }

    fn validate(&self) -> Result<()> {
        if self.extent.is_empty() {
            return Err(Error::Validation(
                "offscreen color extent must be non-zero".into(),
            ));
        }
        if format_has_depth(self.format.to_vk()) || format_has_stencil(self.format.to_vk()) {
            return Err(Error::Validation(
                "offscreen presentation target requires a defined color format".into(),
            ));
        }
        if self.frame_slots == 0 {
            return Err(Error::Validation(
                "offscreen color requires at least one frame slot".into(),
            ));
        }
        Ok(())
    }
}

/// One borrowed per-slot offscreen image/view pair.
#[derive(Clone, Copy, Debug)]
pub struct OffscreenColorTarget<'a> {
    pub image: &'a Image,
    pub view: &'a ImageView,
}

/// Retained, allocator-backed offscreen targets shared by scene, UI and
/// compositor products.
#[derive(Debug)]
pub struct OffscreenColorTargets {
    descriptor: OffscreenColorTargetsDescriptor,
    images: Vec<Image>,
    views: Vec<ImageView>,
}

impl MemoryAllocator {
    pub fn create_offscreen_color_targets(
        &self,
        descriptor: &OffscreenColorTargetsDescriptor,
    ) -> Result<OffscreenColorTargets> {
        descriptor.validate()?;
        let mut images = Vec::with_capacity(descriptor.frame_slots as usize);
        let mut views = Vec::with_capacity(descriptor.frame_slots as usize);
        for frame_slot in 0..descriptor.frame_slots {
            let label = descriptor
                .label
                .as_deref()
                .map(|label| format!("{label}-frame-{frame_slot}"));
            let image = self.create_image(&ImageDescriptor {
                label: label.clone(),
                dimension: crate::ImageDimension::D2,
                format: descriptor.format,
                extent: crate::Extent3D::new(descriptor.extent.width, descriptor.extent.height, 1),
                mip_levels: 1,
                array_layers: 1,
                samples: crate::SampleCount::One,
                tiling: crate::ImageTiling::Optimal,
                usage: crate::TextureUsages::COLOR_ATTACHMENT
                    | crate::TextureUsages::SAMPLED
                    | descriptor.additional_usage,
                memory: MemoryLocation::Device,
            })?;
            let view = image.create_view(&ImageViewDescriptor {
                label,
                dimension: crate::ImageViewDimension::D2,
                format: descriptor.format,
                components: crate::ComponentMapping::default(),
                subresource_range: image.full_subresource_range(crate::TextureAspects::COLOR),
            })?;
            images.push(image);
            views.push(view);
        }
        Ok(OffscreenColorTargets {
            descriptor: descriptor.clone(),
            images,
            views,
        })
    }
}

impl OffscreenColorTargets {
    pub const fn descriptor(&self) -> &OffscreenColorTargetsDescriptor {
        &self.descriptor
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub fn target(&self, frame_slot: usize) -> Result<OffscreenColorTarget<'_>> {
        let image = self.images.get(frame_slot).ok_or_else(|| {
            Error::Validation(format!("offscreen frame slot {frame_slot} is missing"))
        })?;
        let view = self.views.get(frame_slot).ok_or_else(|| {
            Error::Validation(format!("offscreen view slot {frame_slot} is missing"))
        })?;
        Ok(OffscreenColorTarget { image, view })
    }

    /// Immutable views in stable frame-slot order. Consumers that need all
    /// slots during cold graph setup can borrow these without taking ownership
    /// of the renderer-allocated targets.
    pub fn views(&self) -> &[ImageView] {
        &self.views
    }

    pub fn allocation_size(&self) -> u64 {
        self.images.iter().map(Image::allocation_size).sum()
    }
}

/// Sampler descriptor ownership for immutable offscreen frame slots.
///
/// Sharing a sampler minimizes descriptor memory. Per-slot lanes preserve an
/// existing command ABI when every frame slot has historically pushed its own
/// sampler index. Neither topology changes the presentation-path choice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OffscreenSamplerTopology {
    #[default]
    Shared,
    PerFrameSlot,
}

/// One immutable descriptor-heap set for all offscreen frame slots.
#[derive(Debug)]
pub struct OffscreenSampledBindings {
    topology: OffscreenSamplerTopology,
    resource_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    _image_bindings: Vec<SampledImageBinding>,
    _sampler_bindings: Vec<SamplerBinding>,
    indices: Vec<SampledTextureHeapIndices>,
}

impl Backend {
    pub fn create_offscreen_sampled_bindings(
        &self,
        targets: &OffscreenColorTargets,
        sampler: SamplerDescriptor,
    ) -> Result<OffscreenSampledBindings> {
        self.create_offscreen_sampled_bindings_with_topology(
            targets,
            sampler,
            OffscreenSamplerTopology::Shared,
        )
    }

    pub fn create_offscreen_sampled_bindings_with_topology(
        &self,
        targets: &OffscreenColorTargets,
        sampler: SamplerDescriptor,
        topology: OffscreenSamplerTopology,
    ) -> Result<OffscreenSampledBindings> {
        let limits = self.device_info().limits.descriptor_heap;
        let resource_stride = limits.unified_resource_descriptor_stride().ok_or_else(|| {
            Error::Validation(
                "offscreen resource descriptor limits do not satisfy the unified Slang ABI".into(),
            )
        })?;
        let sampler_stride = limits.sampler_descriptor_stride().ok_or_else(|| {
            Error::Validation(
                "offscreen sampler descriptor limits do not satisfy the Slang heap ABI".into(),
            )
        })?;
        let sampler_count = sampler_count(topology, targets.len())?;
        let resource_capacity = resource_stride
            .checked_mul(targets.len() as u64)
            .ok_or_else(|| Error::Validation("offscreen resource heap size overflows".into()))?;
        let sampler_capacity = sampler_stride
            .checked_mul(sampler_count as u64)
            .ok_or_else(|| Error::Validation("offscreen sampler heap size overflows".into()))?;
        let resource_heap = self.create_descriptor_heap(&DescriptorHeapDescriptor {
            label: Some("offscreen-color-resource-heap".into()),
            kind: DescriptorHeapKind::Resource,
            descriptor_capacity: resource_capacity,
            embedded_samplers: false,
        })?;
        let sampler_heap = self.create_descriptor_heap(&DescriptorHeapDescriptor {
            label: Some("offscreen-color-sampler-heap".into()),
            kind: DescriptorHeapKind::Sampler,
            descriptor_capacity: sampler_capacity,
            embedded_samplers: false,
        })?;
        let mut sampler_bindings = Vec::with_capacity(sampler_count);
        for _ in 0..sampler_count {
            sampler_bindings.push(SamplerBinding::new(&sampler_heap, sampler)?);
        }
        let mut image_bindings = Vec::with_capacity(targets.len());
        let mut indices = Vec::with_capacity(targets.len());
        for frame_slot in 0..targets.len() {
            let target = targets.target(frame_slot)?;
            let image = SampledImageBinding::new(
                &resource_heap,
                target.view,
                crate::TextureLayout::ShaderReadOnly,
            )?;
            let sampler_binding = match topology {
                OffscreenSamplerTopology::Shared => &sampler_bindings[0],
                OffscreenSamplerTopology::PerFrameSlot => &sampler_bindings[frame_slot],
            };
            indices.push(SampledTextureHeapIndices::from_bindings(
                &image,
                sampler_binding,
            )?);
            image_bindings.push(image);
        }
        Ok(OffscreenSampledBindings {
            topology,
            resource_heap,
            sampler_heap,
            _image_bindings: image_bindings,
            _sampler_bindings: sampler_bindings,
            indices,
        })
    }
}

impl OffscreenSampledBindings {
    pub const fn topology(&self) -> OffscreenSamplerTopology {
        self.topology
    }

    pub const fn resource_heap(&self) -> &DescriptorHeap {
        &self.resource_heap
    }

    pub const fn sampler_heap(&self) -> &DescriptorHeap {
        &self.sampler_heap
    }

    pub fn indices(&self, frame_slot: usize) -> Result<SampledTextureHeapIndices> {
        self.indices.get(frame_slot).copied().ok_or_else(|| {
            Error::Validation(format!(
                "offscreen sampled binding slot {frame_slot} is missing"
            ))
        })
    }
}

fn sampler_count(topology: OffscreenSamplerTopology, target_count: usize) -> Result<usize> {
    match topology {
        OffscreenSamplerTopology::Shared => (target_count != 0).then_some(1).ok_or_else(|| {
            Error::Validation("offscreen sampled bindings require at least one target".into())
        }),
        OffscreenSamplerTopology::PerFrameSlot => {
            (target_count != 0).then_some(target_count).ok_or_else(|| {
                Error::Validation("offscreen sampled bindings require at least one target".into())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DescriptorHeapLimits;

    use super::*;

    fn requirements() -> PresentationRequirements {
        PresentationRequirements {
            surface_extent: Extent2D::new(3840, 2160),
            target_extent: Extent2D::new(3840, 2160),
            surface_format: TextureFormat::Bgra8Unorm,
            target_format: TextureFormat::Bgra8Unorm,
            frame_slots: 2,
            physical_pass_count: 1,
            sampled_after_write: false,
            has_history: false,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: false,
        }
    }

    fn automatic_policy() -> PresentationPathDescriptor {
        PresentationPathDescriptor {
            target: FrameTargetPreference::Automatic,
            acquire: SurfaceAcquireStrategy::BeforeFrame,
            terminal: TerminalCompositeDescriptor::default(),
        }
    }

    #[test]
    fn automatic_direct_surface_requires_complete_alias_eligibility() {
        let plan = PresentationPathPlan::compile(automatic_policy(), requirements()).unwrap();

        assert_eq!(plan.target, PresentationTarget::DirectSurface);
        assert!(plan.terminal.is_none());
        assert!(plan.direct_surface_blockers.is_empty());
    }

    #[test]
    fn multipass_sampled_opaque_graph_selects_offscreen() {
        let mut requirements = requirements();
        requirements.physical_pass_count = 3;
        requirements.sampled_after_write = true;
        let descriptor = PresentationPathDescriptor {
            terminal: TerminalCompositeDescriptor {
                sampling: TerminalSampling::Linear,
                alpha: TerminalAlphaMode::Opaque,
            },
            ..automatic_policy()
        };

        let plan = PresentationPathPlan::compile(descriptor, requirements).unwrap();

        assert_eq!(plan.target, PresentationTarget::Offscreen);
        assert_eq!(plan.terminal, Some(descriptor.terminal));
        assert_eq!(
            plan.direct_surface_blockers,
            vec![
                DirectSurfaceBlocker::MultiplePhysicalPasses,
                DirectSurfaceBlocker::SampledAfterWrite,
                DirectSurfaceBlocker::TerminalTransform,
            ]
        );
    }

    #[test]
    fn forced_direct_surface_rejects_incompatible_graph() {
        let mut requirements = requirements();
        requirements.has_history = true;
        let descriptor = PresentationPathDescriptor {
            target: FrameTargetPreference::DirectSurface,
            ..automatic_policy()
        };

        let error = PresentationPathPlan::compile(descriptor, requirements).unwrap_err();

        assert!(error.to_string().contains("History"));
    }

    #[test]
    fn late_acquire_requires_offscreen_work() {
        let descriptor = PresentationPathDescriptor {
            acquire: SurfaceAcquireStrategy::AfterOffscreenSubmit,
            ..automatic_policy()
        };
        assert!(PresentationPathPlan::compile(descriptor, requirements()).is_err());

        let descriptor = PresentationPathDescriptor {
            target: FrameTargetPreference::Offscreen,
            acquire: SurfaceAcquireStrategy::AfterOffscreenSubmit,
            ..automatic_policy()
        };
        assert_eq!(
            PresentationPathPlan::compile(descriptor, requirements())
                .unwrap()
                .acquire,
            SurfaceAcquireStrategy::AfterOffscreenSubmit
        );
    }

    #[test]
    fn offscreen_descriptor_only_accepts_offscreen_color_plans() {
        let direct = PresentationPathPlan::compile(automatic_policy(), requirements()).unwrap();
        assert!(OffscreenColorTargetsDescriptor::from_plan(None, &direct).is_err());

        let offscreen = PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::Offscreen,
                ..automatic_policy()
            },
            requirements(),
        )
        .unwrap();
        let descriptor = OffscreenColorTargetsDescriptor::from_plan(None, &offscreen).unwrap();
        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn descriptor_capacity_uses_unified_resource_and_sampler_strides() {
        let limits = DescriptorHeapLimits {
            image_descriptor_size: 32,
            image_descriptor_alignment: 8,
            buffer_descriptor_size: 16,
            buffer_descriptor_alignment: 16,
            sampler_descriptor_size: 16,
            sampler_descriptor_alignment: 8,
            ..DescriptorHeapLimits::default()
        };
        assert_eq!(limits.unified_resource_descriptor_stride(), Some(32));
        assert_eq!(limits.sampler_descriptor_stride(), Some(16));
    }

    #[test]
    fn sampler_topology_keeps_shared_and_per_slot_descriptor_counts_explicit() {
        assert_eq!(
            sampler_count(OffscreenSamplerTopology::Shared, 3).unwrap(),
            1
        );
        assert_eq!(
            sampler_count(OffscreenSamplerTopology::PerFrameSlot, 3).unwrap(),
            3
        );
        assert!(sampler_count(OffscreenSamplerTopology::Shared, 0).is_err());
    }
}
