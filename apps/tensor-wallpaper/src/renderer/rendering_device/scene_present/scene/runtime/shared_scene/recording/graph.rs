//! Allocation-free authored scene-color/effect-pass interleaving.

use vulkan_renderer::{
    ColorAttachment, CommandEncoder, Extent2D, Image, ImageView, LoadOp, Rect2D,
    RenderingDescriptor, ResolveMode, StoreOp, TextureLayout, TextureState,
};

use crate::renderer::rendering_device::RenderingDeviceClearColor;

use super::super::super::draw_recording::{SceneGpuDrawCommand, SceneGpuDrawRange};
use super::super::super::effect_target::{
    SharedSceneEffectCommand, SharedSceneEffectCommandKind, SharedSceneEffectCopySource,
    SharedSceneEffectExecutionPlan,
};
use super::super::super::gpu_timing::SceneGpuTimingFrame;
use super::super::super::local_read::SceneLocalReadScopePassRole;
use super::super::SharedSceneGpuResources;
use super::super::execution_plan::{SharedSceneFrameStep, SharedSceneGraphExecution};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SceneColorState {
    Undefined,
    Sampled,
    Attachment,
}

impl SharedSceneGpuResources {
    /// Records the complete cold-compiled frame graph into one frame-slot
    /// SceneColor image without rebuilding topology or allocating host arrays.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn record_graphs_to_scene_color(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        scene_color: &Image,
        scene_color_view: &ImageView,
        extent: Extent2D,
        reference_phase: usize,
        scene_color_initialized: bool,
        clear_color: RenderingDeviceClearColor,
        gpu_timing: Option<SceneGpuTimingFrame<'_>>,
    ) -> Result<(), String> {
        if extent.is_empty() {
            return Err("shared scene frame has an empty SceneColor extent".into());
        }
        self.bind_frame_heaps(encoder, frame_slot, reference_phase)?;
        if let Some(timing) = gpu_timing {
            timing.start_effect_batches(encoder)?;
        }
        self.record_effect_batches(encoder, frame_slot, reference_phase)?;
        if let Some(timing) = gpu_timing {
            timing.finish_effect_batches(encoder)?;
        }
        let phase = self.effect_execution_plan(reference_phase)?;
        let mut state = if scene_color_initialized {
            SceneColorState::Sampled
        } else {
            SceneColorState::Undefined
        };
        let mut initialized = false;

        for (graph_position, graph) in self.frame_execution_plan.graphs.iter().enumerate() {
            if let Some(timing) = gpu_timing {
                timing.start_graph(encoder, graph_position)?;
            }
            let graph_active = graph_is_active(graph, &self.draw_commands);
            for (pass_position, step) in graph.steps.iter().enumerate() {
                if let Some(timing) = gpu_timing {
                    timing.start_pass(encoder, graph_position, pass_position)?;
                }
                match (graph_active, *step) {
                    (false, _) => {}
                    (true, SharedSceneFrameStep::SceneColor(range)) => {
                        if draw_range_has_enabled(&self.draw_commands, range) {
                            ensure_attachment(encoder, scene_color, &mut state)?;
                            record_scene_color_pass(
                                self,
                                encoder,
                                frame_slot,
                                scene_color_view,
                                extent,
                                graph.graph_index,
                                range,
                                initialized,
                                clear_color,
                            )?;
                            initialized = true;
                        }
                    }
                    (true, SharedSceneFrameStep::Effect { command_index }) => {
                        let command = phase_command(phase, command_index)?;
                        self.record_frame_effect_command(
                            encoder,
                            frame_slot,
                            scene_color,
                            scene_color_view,
                            extent,
                            command,
                            &mut state,
                            &mut initialized,
                            clear_color,
                        )?;
                    }
                    (
                        true,
                        SharedSceneFrameStep::LocalReadPair {
                            producer_command_index,
                            consumer_command_index,
                        },
                    ) => {
                        let producer = phase_command(phase, producer_command_index)?;
                        let consumer = phase_command(phase, consumer_command_index)?;
                        let SharedSceneEffectCommandKind::DynamicRender {
                            local_read: Some((scope_index, SceneLocalReadScopePassRole::Producer)),
                            ..
                        } = producer.kind
                        else {
                            return Err("shared frame local-read producer shape changed".into());
                        };
                        self.record_local_read_pair(
                            encoder,
                            frame_slot,
                            scope_index,
                            producer,
                            consumer,
                        )?;
                    }
                }
                if let Some(timing) = gpu_timing {
                    timing.finish_pass(encoder, graph_position, pass_position)?;
                }
            }
            if let Some(timing) = gpu_timing {
                timing.finish_graph(encoder, graph_position)?;
            }
        }
        ensure_attachment(encoder, scene_color, &mut state)?;
        if !initialized {
            clear_scene_color(encoder, scene_color_view, extent, clear_color)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_frame_effect_command(
        &self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        scene_color: &Image,
        scene_color_view: &ImageView,
        extent: Extent2D,
        command: SharedSceneEffectCommand,
        state: &mut SceneColorState,
        initialized: &mut bool,
        clear_color: RenderingDeviceClearColor,
    ) -> Result<(), String> {
        match command.kind {
            SharedSceneEffectCommandKind::Copy {
                source,
                destination_physical_slot,
                direct_scene_color_snapshot,
                coverage,
            } => {
                if matches!(source, SharedSceneEffectCopySource::SceneColor) && !*initialized {
                    ensure_attachment(encoder, scene_color, state)?;
                    clear_scene_color(encoder, scene_color_view, extent, clear_color)?;
                    *initialized = true;
                }
                if direct_scene_color_snapshot {
                    ensure_sampled(encoder, scene_color, state)
                } else {
                    if matches!(source, SharedSceneEffectCopySource::SceneColor) {
                        ensure_attachment(encoder, scene_color, state)?;
                    }
                    self.record_effect_copy(
                        encoder,
                        scene_color,
                        extent,
                        source,
                        destination_physical_slot.ok_or_else(|| {
                            "shared authored copy has no physical destination".to_owned()
                        })?,
                        coverage,
                    )
                }
            }
            SharedSceneEffectCommandKind::SwapReferences { .. } => Ok(()),
            SharedSceneEffectCommandKind::DynamicRender {
                local_read: Some((scope_index, _)),
                ..
            } => Err(format!(
                "shared frame schedule emitted unpaired local-read scope {scope_index}"
            )),
            SharedSceneEffectCommandKind::DynamicRender { .. } => {
                if let SharedSceneEffectCommandKind::DynamicRender {
                    draw_start,
                    draw_count,
                    ..
                } = command.kind
                    && draw_count != 0
                    && !draw_range_has_enabled(
                        &self.draw_commands,
                        SceneGpuDrawRange {
                            start: draw_start,
                            count: draw_count,
                        },
                    )
                {
                    return Ok(());
                }
                self.record_effect_render(encoder, frame_slot, command)
            }
        }
    }
}

fn phase_command(
    phase: &SharedSceneEffectExecutionPlan,
    command_index: usize,
) -> Result<SharedSceneEffectCommand, String> {
    phase
        .commands
        .get(command_index)
        .copied()
        .ok_or_else(|| format!("shared effect phase is missing command {command_index}"))
}

fn graph_is_active(graph: &SharedSceneGraphExecution, draws: &[SceneGpuDrawCommand]) -> bool {
    let mut has_draw = false;
    let any_visible = graph.draw_ranges.iter().any(|range| {
        let start = range.start as usize;
        let end = start.saturating_add(range.count as usize);
        draws.get(start..end).into_iter().flatten().any(|draw| {
            has_draw = true;
            draw.enabled
        })
    });
    any_visible
        || (!has_draw
            && graph.activation_policy
                == crate::engine::scene::SceneRenderGraphActivationPolicy::Always)
}

fn draw_range_has_enabled(draws: &[SceneGpuDrawCommand], range: SceneGpuDrawRange) -> bool {
    let start = range.start as usize;
    let end = start.saturating_add(range.count as usize);
    draws
        .get(start..end)
        .is_some_and(|commands| commands.iter().any(|draw| draw.enabled))
}

fn ensure_attachment(
    encoder: &mut CommandEncoder,
    scene_color: &Image,
    state: &mut SceneColorState,
) -> Result<(), String> {
    let old = match *state {
        SceneColorState::Undefined => TextureState::Undefined,
        SceneColorState::Sampled => TextureState::FragmentSampledRead,
        SceneColorState::Attachment => return Ok(()),
    };
    encoder
        .transition_image(scene_color, old, TextureState::ColorAttachmentWrite)
        .map_err(|error| format!("transition shared SceneColor for rendering: {error}"))?;
    *state = SceneColorState::Attachment;
    Ok(())
}

fn ensure_sampled(
    encoder: &mut CommandEncoder,
    scene_color: &Image,
    state: &mut SceneColorState,
) -> Result<(), String> {
    match *state {
        SceneColorState::Sampled => return Ok(()),
        SceneColorState::Undefined => {
            return Err("shared SceneColor cannot be sampled before initialization".into());
        }
        SceneColorState::Attachment => {}
    }
    encoder
        .transition_image(
            scene_color,
            TextureState::ColorAttachmentWrite,
            TextureState::FragmentSampledRead,
        )
        .map_err(|error| format!("transition shared SceneColor for sampling: {error}"))?;
    *state = SceneColorState::Sampled;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_scene_color_pass(
    scene: &SharedSceneGpuResources,
    encoder: &mut CommandEncoder,
    frame_slot: usize,
    scene_color_view: &ImageView,
    extent: Extent2D,
    graph_index: u32,
    range: SceneGpuDrawRange,
    initialized: bool,
    clear_color: RenderingDeviceClearColor,
) -> Result<(), String> {
    let replacement = scene.scene_color_attachment_clear.filter(|clear| {
        clear.replaces(
            super::super::super::draw_recording::SceneGpuGraphDrawRange { graph_index, range },
        )
    });
    let load_op = if let Some(clear) = replacement {
        LoadOp::Clear(clear_color_array(clear.color))
    } else if initialized {
        LoadOp::Load
    } else {
        LoadOp::Clear(clear_color_array(clear_color))
    };
    let attachments = [Some(scene_color_attachment(scene_color_view, load_op))];
    let descriptor = scene_color_descriptor(extent, &attachments);
    unsafe {
        let mut rendering = encoder
            .begin_rendering(&descriptor)
            .map_err(|error| format!("begin shared SceneColor pass: {error}"))?;
        rendering.retain_resource(scene_color_view);
        if replacement.is_none() {
            scene.record_draw_range(&mut rendering, frame_slot, range, extent)?;
        }
    }
    Ok(())
}

fn clear_scene_color(
    encoder: &mut CommandEncoder,
    scene_color_view: &ImageView,
    extent: Extent2D,
    clear_color: RenderingDeviceClearColor,
) -> Result<(), String> {
    let attachments = [Some(scene_color_attachment(
        scene_color_view,
        LoadOp::Clear(clear_color_array(clear_color)),
    ))];
    let descriptor = scene_color_descriptor(extent, &attachments);
    encoder.retain_resource(scene_color_view);
    unsafe {
        encoder
            .begin_rendering(&descriptor)
            .map_err(|error| format!("clear shared SceneColor: {error}"))?;
    }
    Ok(())
}

fn scene_color_attachment(
    scene_color_view: &ImageView,
    load_op: LoadOp<[f32; 4]>,
) -> ColorAttachment<'_> {
    ColorAttachment {
        view: scene_color_view.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op,
        store_op: StoreOp::Store,
    }
}

fn scene_color_descriptor<'a>(
    extent: Extent2D,
    attachments: &'a [Option<ColorAttachment<'a>>],
) -> RenderingDescriptor<'a> {
    RenderingDescriptor {
        label: Some("tensor-wallpaper-scene-color-pass"),
        render_area: Rect2D::new(0, 0, extent.width, extent.height),
        layer_count: 1,
        view_mask: 0,
        color_attachments: attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    }
}

const fn clear_color_array(color: RenderingDeviceClearColor) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}
