use vulkan_renderer::{
    Extent2D, FrameTargetPreference, PresentationPathDescriptor, PresentationPathPlan,
    PresentationRequirements, Rect2D, SurfaceAcquireStrategy, TerminalAlphaMode,
    TerminalCompositeDescriptor, TerminalSampling, TextureFormat, TextureUsages,
};

use crate::SurfaceKey;

/// Product semantics retained before frame topology is selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellRenderNode {
    pub effect: ShellEffect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellEffect {
    /// Ordinary chrome, text, icons, shadows, and other analytic draws.
    Direct,
    /// A bounded effect that samples already-composited scene color.
    SceneColor {
        output_region: Rect2D,
        sample_region: Rect2D,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellRenderScene {
    pub surface: SurfaceKey,
    nodes: Vec<ShellRenderNode>,
    pub has_history: bool,
    pub has_external_consumer: bool,
    pub uses_async_compute: bool,
    pub requires_terminal_transform: bool,
}

impl ShellRenderScene {
    pub const fn new(surface: SurfaceKey) -> Self {
        Self {
            surface,
            nodes: Vec::new(),
            has_history: false,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: false,
        }
    }

    pub fn push(&mut self, node: ShellRenderNode) {
        self.nodes.push(node);
    }

    pub fn nodes(&self) -> &[ShellRenderNode] {
        &self.nodes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSceneColorPass {
    pub node_index: u32,
    pub output_region: Rect2D,
    pub sample_region: Rect2D,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellCompositionPath {
    DirectSinglePass,
    DirectWithLocalPasses(Vec<LocalSceneColorPass>),
    OffscreenMultiPass {
        local_passes: Vec<LocalSceneColorPass>,
    },
}

/// Product-local pass plan plus the shared renderer's presentation decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellFramePlan {
    pub surface: SurfaceKey,
    pub presentation: PresentationPathPlan,
    pub composition: ShellCompositionPath,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ShellFramePlanError {
    #[error("Tensor Shell surface extent must be non-empty")]
    EmptySurfaceExtent,
    #[error("Tensor Shell scene-color node {node_index} has an empty {region} region")]
    EmptySceneColorRegion {
        node_index: usize,
        region: &'static str,
    },
    #[error("Tensor Shell scene-color node {node_index} has an out-of-bounds {region} region")]
    SceneColorRegionOutOfBounds {
        node_index: usize,
        region: &'static str,
    },
    #[error("Tensor Shell scene node index overflows u32")]
    NodeIndexOverflow,
    #[error("compile Tensor Shell presentation path: {0}")]
    Presentation(String),
}

/// Lowers Tensor Shell scene semantics into product-local pass topology and
/// generic presentation facts. The shared renderer decides direct versus
/// offscreen presentation; local effect order and regions remain shell-owned.
pub fn compile_frame_plan(
    scene: &ShellRenderScene,
    extent: Extent2D,
    format: TextureFormat,
    surface_usage: TextureUsages,
) -> Result<ShellFramePlan, ShellFramePlanError> {
    if extent.is_empty() {
        return Err(ShellFramePlanError::EmptySurfaceExtent);
    }
    let local_passes = scene
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node_index, node)| match node.effect {
            ShellEffect::Direct => None,
            ShellEffect::SceneColor {
                output_region,
                sample_region,
            } => Some(validate_local_pass(
                node_index,
                output_region,
                sample_region,
                extent,
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let local_passes_require_offscreen =
        !local_passes.is_empty() && !surface_usage.contains(TextureUsages::COPY_SOURCE);
    let presentation = PresentationPathPlan::compile(
        PresentationPathDescriptor {
            target: FrameTargetPreference::Automatic,
            acquire: SurfaceAcquireStrategy::BeforeFrame,
            terminal: TerminalCompositeDescriptor {
                sampling: TerminalSampling::Nearest,
                alpha: TerminalAlphaMode::Preserve,
            },
        },
        PresentationRequirements {
            surface_extent: extent,
            target_extent: extent,
            surface_format: format,
            target_format: format,
            frame_slots: 3,
            physical_pass_count: 1 + u32::from(local_passes_require_offscreen),
            sampled_after_write: local_passes_require_offscreen,
            has_history: scene.has_history,
            has_external_consumer: scene.has_external_consumer,
            uses_async_compute: scene.uses_async_compute,
            requires_terminal_transform: scene.requires_terminal_transform,
        },
    )
    .map_err(|error| ShellFramePlanError::Presentation(error.to_string()))?;
    let composition = match presentation.target {
        vulkan_renderer::PresentationTarget::Offscreen => {
            ShellCompositionPath::OffscreenMultiPass { local_passes }
        }
        vulkan_renderer::PresentationTarget::DirectSurface if local_passes.is_empty() => {
            ShellCompositionPath::DirectSinglePass
        }
        vulkan_renderer::PresentationTarget::DirectSurface => {
            ShellCompositionPath::DirectWithLocalPasses(local_passes)
        }
    };
    Ok(ShellFramePlan {
        surface: scene.surface,
        presentation,
        composition,
    })
}

fn validate_local_pass(
    node_index: usize,
    output_region: Rect2D,
    sample_region: Rect2D,
    surface_extent: Extent2D,
) -> Result<LocalSceneColorPass, ShellFramePlanError> {
    validate_region(node_index, "output", output_region, surface_extent)?;
    validate_region(node_index, "sample", sample_region, surface_extent)?;
    Ok(LocalSceneColorPass {
        node_index: u32::try_from(node_index)
            .map_err(|_| ShellFramePlanError::NodeIndexOverflow)?,
        output_region,
        sample_region,
    })
}

fn validate_region(
    node_index: usize,
    region_name: &'static str,
    region: Rect2D,
    surface_extent: Extent2D,
) -> Result<(), ShellFramePlanError> {
    if region.extent.is_empty() {
        return Err(ShellFramePlanError::EmptySceneColorRegion {
            node_index,
            region: region_name,
        });
    }
    let in_bounds = u32::try_from(region.origin.x)
        .ok()
        .zip(u32::try_from(region.origin.y).ok())
        .and_then(|(x, y)| {
            x.checked_add(region.extent.width)
                .zip(y.checked_add(region.extent.height))
        })
        .is_some_and(|(right, bottom)| {
            right <= surface_extent.width && bottom <= surface_extent.height
        });
    if !in_bounds {
        return Err(ShellFramePlanError::SceneColorRegionOutOfBounds {
            node_index,
            region: region_name,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShellComponent, SurfaceKey};
    use wayland_client_runtime::OutputId;

    fn key(component: ShellComponent) -> SurfaceKey {
        SurfaceKey {
            output: OutputId::from_raw(1),
            component,
        }
    }

    fn compile(scene: &ShellRenderScene, usage: TextureUsages) -> ShellFramePlan {
        compile_frame_plan(
            scene,
            Extent2D::new(800, 600),
            TextureFormat::Bgra8Unorm,
            usage,
        )
        .unwrap()
    }

    #[test]
    fn ordinary_shell_scene_is_direct_single_pass() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::Panel));
        scene.push(ShellRenderNode {
            effect: ShellEffect::Direct,
        });
        let plan = compile(&scene, TextureUsages::COLOR_ATTACHMENT);
        assert_eq!(plan.composition, ShellCompositionPath::DirectSinglePass);
        assert_eq!(
            plan.presentation.target,
            vulkan_renderer::PresentationTarget::DirectSurface
        );
    }

    #[test]
    fn bounded_scene_color_dependency_stays_local_when_surface_can_be_copied() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::ControlCenter));
        scene.push(ShellRenderNode {
            effect: ShellEffect::SceneColor {
                output_region: Rect2D::new(600, 40, 180, 400),
                sample_region: Rect2D::new(588, 28, 204, 424),
            },
        });
        let plan = compile(
            &scene,
            TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
        );
        assert!(matches!(
            plan.composition,
            ShellCompositionPath::DirectWithLocalPasses(ref passes) if passes.len() == 1
        ));
        assert_eq!(
            plan.presentation.target,
            vulkan_renderer::PresentationTarget::DirectSurface
        );
    }

    #[test]
    fn local_dependency_selects_shared_offscreen_path_without_copy_source() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::Overview));
        scene.push(ShellRenderNode {
            effect: ShellEffect::SceneColor {
                output_region: Rect2D::new(0, 0, 800, 600),
                sample_region: Rect2D::new(0, 0, 800, 600),
            },
        });
        let plan = compile(&scene, TextureUsages::COLOR_ATTACHMENT);
        assert!(matches!(
            plan.composition,
            ShellCompositionPath::OffscreenMultiPass { ref local_passes }
                if local_passes.len() == 1
        ));
        assert_eq!(
            plan.presentation.target,
            vulkan_renderer::PresentationTarget::Offscreen
        );
    }

    #[test]
    fn global_offscreen_fact_preserves_product_local_passes() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::ControlCenter));
        scene.has_history = true;
        scene.push(ShellRenderNode {
            effect: ShellEffect::SceneColor {
                output_region: Rect2D::new(600, 40, 180, 400),
                sample_region: Rect2D::new(588, 28, 204, 424),
            },
        });
        let plan = compile(
            &scene,
            TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
        );
        assert!(matches!(
            plan.composition,
            ShellCompositionPath::OffscreenMultiPass { ref local_passes }
                if local_passes.len() == 1
        ));
        assert_eq!(
            plan.presentation.target,
            vulkan_renderer::PresentationTarget::Offscreen
        );
    }

    #[test]
    fn empty_surface_extent_is_rejected_before_presentation_planning() {
        let scene = ShellRenderScene::new(key(ShellComponent::Panel));
        assert_eq!(
            compile_frame_plan(
                &scene,
                Extent2D::new(800, 0),
                TextureFormat::Bgra8Unorm,
                TextureUsages::COLOR_ATTACHMENT,
            ),
            Err(ShellFramePlanError::EmptySurfaceExtent)
        );
    }

    #[test]
    fn scene_color_output_must_fit_the_surface() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::ControlCenter));
        scene.push(ShellRenderNode {
            effect: ShellEffect::SceneColor {
                output_region: Rect2D::new(790, 20, 20, 40),
                sample_region: Rect2D::new(780, 10, 20, 60),
            },
        });
        assert_eq!(
            compile_frame_plan(
                &scene,
                Extent2D::new(800, 600),
                TextureFormat::Bgra8Unorm,
                TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
            ),
            Err(ShellFramePlanError::SceneColorRegionOutOfBounds {
                node_index: 0,
                region: "output",
            })
        );
    }

    #[test]
    fn scene_color_sample_rejects_negative_origin() {
        let mut scene = ShellRenderScene::new(key(ShellComponent::Overview));
        scene.push(ShellRenderNode {
            effect: ShellEffect::SceneColor {
                output_region: Rect2D::new(0, 0, 800, 600),
                sample_region: Rect2D::new(-1, 0, 800, 600),
            },
        });
        assert_eq!(
            compile_frame_plan(
                &scene,
                Extent2D::new(800, 600),
                TextureFormat::Bgra8Unorm,
                TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
            ),
            Err(ShellFramePlanError::SceneColorRegionOutOfBounds {
                node_index: 0,
                region: "sample",
            })
        );
    }
}
