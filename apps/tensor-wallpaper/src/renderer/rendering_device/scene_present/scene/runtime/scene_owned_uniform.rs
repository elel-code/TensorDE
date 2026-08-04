//! Retained, aligned uniform-buffer storage for scene-owned graphics programs.

use crate::engine::scene::semantic_world::ResolvedMaterialScalarValue;
use crate::engine::scene::{
    SceneMaterialHandle, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw,
    SceneRenderingDeviceProjectionDomain, SceneStorage, StereoSpectrum64,
};
use serde::Serialize;

use super::ScenePipelineDescriptorLayout;
use super::effect_target::SceneEffectTargetImagePlan;
use super::material_uniform::{material_pass_constants, parse_constant_values};
use super::sampled_binding::{SceneSampledImageBindingPlan, SceneSampledImageSource};
use super::scene_viewport::{apply_scene_cover_clip_scale, scene_cover_clip_scale};
use super::shader_program::{
    SceneAudioSpectrumChannel, SceneAudioSpectrumResolution, SceneOwnedUniformSource,
    SceneResolvedGraphicsProgram, resolve_scene_graphics_program, scene_owned_stage_resource_plan,
};
use super::shader_uniform::inverse_affine_rows;

mod audio_spectrum;
mod payload;

#[cfg(test)]
mod tests;

use audio_spectrum::write_audio_spectrum;
use payload::{write_matrix, write_values};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneOwnedUniformArenaPlan {
    pub(super) byte_count: u64,
    slices: Vec<SceneOwnedUniformSlicePlan>,
    sampled_slots: Vec<u32>,
    phase_resolutions: Vec<Vec<[f32; 4]>>,
    scene_cover_clip_scale: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedUniformFrameInputs<'a> {
    pub scalar_overrides: &'a [ResolvedMaterialScalarValue],
    pub scene_time_seconds: f32,
    pub frame_delta_seconds: f32,
    pub audio_spectrum: &'a StereoSpectrum64,
    pub parallax_position: [f32; 2],
    pub sampled_binding_phase: usize,
}

impl SceneOwnedUniformFrameInputs<'static> {
    pub(super) const INITIAL: Self = Self {
        scalar_overrides: &[],
        scene_time_seconds: 0.0,
        frame_delta_seconds: 0.0,
        audio_spectrum: &StereoSpectrum64::ZERO,
        parallax_position: [0.5; 2],
        sampled_binding_phase: 0,
    };
}

#[derive(Debug, Clone, PartialEq)]
struct SceneOwnedUniformSlicePlan {
    draw_index: usize,
    descriptor_lane: usize,
    byte_offset: u64,
    byte_size: u32,
    members: Vec<SceneOwnedUniformMemberSource>,
}

#[derive(Debug, Clone, PartialEq)]
struct SceneOwnedUniformMemberSource {
    byte_offset: u32,
    byte_size: u32,
    array_stride: u32,
    source: SceneOwnedRetainedSource,
}

#[derive(Debug, Clone, PartialEq)]
enum SceneOwnedRetainedSource {
    SceneTime,
    FrameDelta,
    AudioSpectrum {
        channel: SceneAudioSpectrumChannel,
        resolution: SceneAudioSpectrumResolution,
    },
    ModelViewProjectionMatrix,
    EffectModelViewProjectionMatrix,
    EffectTextureProjectionMatrixInverse,
    LayerModelMatrix,
    ObjectColor4,
    ObjectAlpha,
    ParallaxPosition,
    CurrentRenderTargetTexelSize {
        texel_size: [f32; 2],
    },
    SampledTextureResolution {
        sampled_slot_index: usize,
    },
    MaterialConstant {
        constant_index: u32,
        default_values: Vec<f32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneOwnedUniformArenaPlanSnapshot {
    pub output_extent: [u32; 2],
    pub min_uniform_buffer_offset_alignment: u64,
    pub byte_count: u64,
    pub slice_count: usize,
    pub sampled_binding_phase_count: usize,
    pub initial_payload_byte_count: usize,
    pub descriptor_slices: Vec<RenderingDeviceSceneOwnedUniformSliceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneOwnedUniformSliceSnapshot {
    pub draw_index: usize,
    pub descriptor_lane: usize,
    pub byte_offset: u64,
    pub byte_size: u64,
}

pub fn rendering_device_scene_owned_uniform_arena_plan(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    output_extent: [u32; 2],
    min_uniform_buffer_offset_alignment: u64,
) -> Result<RenderingDeviceSceneOwnedUniformArenaPlanSnapshot, String> {
    if output_extent.contains(&0) {
        return Err("scene-owned uniform plan output extent must be non-zero".to_owned());
    }
    let descriptor_layout = super::scene_pipeline_descriptor_layout(storage, graph)?;
    let sampled_binding_cycle = super::scene_sampled_image_binding_cycle(
        graph,
        &descriptor_layout.sampled_slots,
        &descriptor_layout.input_attachment_slots,
    )?;
    let effect_targets = super::effect_target::scene_effect_target_image_plan(
        storage,
        graph,
        vulkan_renderer::TextureFormat::Bgra8Srgb,
        vulkan_renderer::Extent2D::new(output_extent[0], output_extent[1]),
    )?;
    let plan = SceneOwnedUniformArenaPlan::build(
        storage,
        graph,
        &descriptor_layout,
        &sampled_binding_cycle,
        &effect_targets,
        output_extent,
        min_uniform_buffer_offset_alignment,
    )?;
    let payload_byte_count = usize::try_from(plan.byte_count)
        .map_err(|_| "scene-owned uniform plan exceeds host address space".to_owned())?;
    let mut initial_payload = vec![0; payload_byte_count];
    if !plan.is_empty() {
        plan.write_payload(
            &graph.mesh_draws,
            SceneOwnedUniformFrameInputs::INITIAL,
            &mut initial_payload,
        )?;
    }
    let descriptor_slices = plan
        .descriptor_slices()
        .map(|(draw_index, descriptor_lane, byte_offset, byte_size)| {
            RenderingDeviceSceneOwnedUniformSliceSnapshot {
                draw_index,
                descriptor_lane,
                byte_offset,
                byte_size,
            }
        })
        .collect::<Vec<_>>();
    Ok(RenderingDeviceSceneOwnedUniformArenaPlanSnapshot {
        output_extent,
        min_uniform_buffer_offset_alignment,
        byte_count: plan.byte_count,
        slice_count: descriptor_slices.len(),
        sampled_binding_phase_count: sampled_binding_cycle.len(),
        initial_payload_byte_count: initial_payload.len(),
        descriptor_slices,
    })
}

impl SceneOwnedUniformArenaPlan {
    pub(super) fn payload_slices_for_draw<'a>(
        &self,
        draw_index: usize,
        payload: &'a [u8],
    ) -> Result<Vec<&'a [u8]>, String> {
        let expected_len = usize::try_from(self.byte_count)
            .map_err(|_| "scene-owned diagnostic payload size exceeds usize".to_owned())?;
        if payload.len() != expected_len {
            return Err(format!(
                "scene-owned diagnostic payload has {} bytes, expected {}",
                payload.len(),
                self.byte_count
            ));
        }
        self.slices
            .iter()
            .filter(|slice| slice.draw_index == draw_index)
            .map(|slice| {
                let start = usize::try_from(slice.byte_offset)
                    .map_err(|_| "scene-owned diagnostic slice offset exceeds usize".to_owned())?;
                let end = start
                    .checked_add(slice.byte_size as usize)
                    .ok_or_else(|| "scene-owned diagnostic slice range overflows".to_owned())?;
                payload.get(start..end).ok_or_else(|| {
                    format!(
                        "scene-owned diagnostic slice {start}..{end} exceeds {} bytes",
                        payload.len()
                    )
                })
            })
            .collect()
    }

    pub(super) fn build(
        storage: &SceneStorage,
        graph: &SceneRenderingDeviceGraphPlan,
        descriptor_layout: &ScenePipelineDescriptorLayout,
        sampled_binding_cycle: &[SceneSampledImageBindingPlan],
        effect_targets: &[SceneEffectTargetImagePlan],
        output_extent: [u32; 2],
        min_uniform_buffer_offset_alignment: u64,
    ) -> Result<Self, String> {
        if min_uniform_buffer_offset_alignment == 0 {
            return Err("scene-owned uniform alignment must be non-zero".to_owned());
        }
        let mut slices = Vec::new();
        let mut byte_cursor = 0u64;
        for (draw_index, draw) in graph.mesh_draws.iter().enumerate() {
            let SceneResolvedGraphicsProgram::SceneOwned {
                vertex, fragment, ..
            } = resolve_scene_graphics_program(storage, draw.shader_key, draw.primitive)?
            else {
                continue;
            };
            let vertex = scene_owned_stage_resource_plan(storage, vertex)?;
            let fragment = scene_owned_stage_resource_plan(storage, fragment)?;
            let mut descriptor_lane = 0usize;
            for stage in [&vertex, &fragment] {
                for buffer in &stage.uniform_buffers {
                    if descriptor_lane >= descriptor_layout.scene_owned_uniform_count {
                        return Err(format!(
                            "scene-owned draw {draw_index} uniform lane {descriptor_lane} exceeds retained descriptor capacity {}",
                            descriptor_layout.scene_owned_uniform_count
                        ));
                    }
                    byte_cursor = align_up(
                        byte_cursor,
                        min_uniform_buffer_offset_alignment,
                        "scene-owned uniform slice",
                    )?;
                    let members = buffer
                        .members
                        .iter()
                        .map(|member| {
                            let source = match member.source {
                                SceneOwnedUniformSource::SceneTime => {
                                    SceneOwnedRetainedSource::SceneTime
                                }
                                SceneOwnedUniformSource::FrameDelta => {
                                    SceneOwnedRetainedSource::FrameDelta
                                }
                                SceneOwnedUniformSource::AudioSpectrum {
                                    channel,
                                    resolution,
                                } => SceneOwnedRetainedSource::AudioSpectrum {
                                    channel,
                                    resolution,
                                },
                                SceneOwnedUniformSource::ModelViewProjectionMatrix => {
                                    SceneOwnedRetainedSource::ModelViewProjectionMatrix
                                }
                                SceneOwnedUniformSource::EffectModelViewProjectionMatrix => {
                                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix
                                }
                                SceneOwnedUniformSource::EffectTextureProjectionMatrixInverse => {
                                    SceneOwnedRetainedSource::EffectTextureProjectionMatrixInverse
                                }
                                SceneOwnedUniformSource::LayerModelMatrix => {
                                    SceneOwnedRetainedSource::LayerModelMatrix
                                }
                                SceneOwnedUniformSource::ObjectColor4 => {
                                    SceneOwnedRetainedSource::ObjectColor4
                                }
                                SceneOwnedUniformSource::ObjectAlpha => {
                                    SceneOwnedRetainedSource::ObjectAlpha
                                }
                                SceneOwnedUniformSource::ParallaxPosition => {
                                    SceneOwnedRetainedSource::ParallaxPosition
                                }
                                SceneOwnedUniformSource::CurrentRenderTargetTexelSize => {
                                    SceneOwnedRetainedSource::CurrentRenderTargetTexelSize {
                                        texel_size: current_render_target_texel_size(
                                            storage,
                                            graph,
                                            draw_index,
                                            output_extent,
                                        )?,
                                    }
                                }
                                SceneOwnedUniformSource::SampledTextureResolution { slot } => {
                                    let sampled_slot_index = descriptor_layout
                                        .sampled_slots
                                        .iter()
                                        .position(|candidate| *candidate == slot)
                                        .ok_or_else(|| {
                                            format!(
                                                "scene-owned draw {draw_index} uniform {:?} uses unplanned sampled slot {slot}",
                                                member.name
                                            )
                                        })?;
                                    SceneOwnedRetainedSource::SampledTextureResolution {
                                        sampled_slot_index,
                                    }
                                }
                                SceneOwnedUniformSource::MaterialParameter { authored_name } => {
                                    let (constant_index, default_values) = material_constant_source(
                                        storage,
                                        draw.material,
                                        authored_name,
                                    )?;
                                    let default_bytes = default_values
                                        .len()
                                        .checked_mul(size_of::<f32>())
                                        .ok_or_else(|| {
                                            "scene-owned material default size overflows".to_owned()
                                        })?;
                                    if default_bytes != member.byte_size as usize {
                                        return Err(format!(
                                            "scene-owned material parameter {authored_name:?} has {default_bytes} default bytes, uniform {:?} requires {}",
                                            member.name, member.byte_size
                                        ));
                                    }
                                    SceneOwnedRetainedSource::MaterialConstant {
                                        constant_index,
                                        default_values,
                                    }
                                }
                            };
                            Ok(SceneOwnedUniformMemberSource {
                                byte_offset: member.byte_offset,
                                byte_size: member.byte_size,
                                array_stride: member.array_stride,
                                source,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    slices.push(SceneOwnedUniformSlicePlan {
                        draw_index,
                        descriptor_lane,
                        byte_offset: byte_cursor,
                        byte_size: buffer.byte_size,
                        members,
                    });
                    byte_cursor = byte_cursor
                        .checked_add(u64::from(buffer.byte_size))
                        .ok_or_else(|| "scene-owned uniform arena size overflows".to_owned())?;
                    descriptor_lane += 1;
                }
            }
        }
        if slices.is_empty() {
            return Ok(Self {
                byte_count: 0,
                slices,
                sampled_slots: descriptor_layout.sampled_slots.clone(),
                phase_resolutions: Vec::new(),
                scene_cover_clip_scale: scene_cover_clip_scale(storage.project(), output_extent),
            });
        }
        let phase_resolutions = sampled_binding_cycle
            .iter()
            .map(|phase| {
                phase_resolutions(
                    storage,
                    graph.mesh_draws.len(),
                    descriptor_layout,
                    phase,
                    effect_targets,
                    output_extent,
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !slices.is_empty() && phase_resolutions.is_empty() {
            return Err("scene-owned uniform arena has no sampled binding phase".to_owned());
        }
        Ok(Self {
            byte_count: byte_cursor,
            slices,
            sampled_slots: descriptor_layout.sampled_slots.clone(),
            phase_resolutions,
            scene_cover_clip_scale: scene_cover_clip_scale(storage.project(), output_extent),
        })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    pub(super) fn descriptor_slices(&self) -> impl Iterator<Item = (usize, usize, u64, u64)> + '_ {
        self.slices.iter().map(|slice| {
            (
                slice.draw_index,
                slice.descriptor_lane,
                slice.byte_offset,
                u64::from(slice.byte_size),
            )
        })
    }

    pub(super) fn write_payload(
        &self,
        draws: &[SceneRenderingDeviceMeshDraw],
        inputs: SceneOwnedUniformFrameInputs<'_>,
        output: &mut [u8],
    ) -> Result<(), String> {
        if output.len() as u64 != self.byte_count {
            return Err(format!(
                "scene-owned uniform arena requires {} bytes, received {}",
                self.byte_count,
                output.len()
            ));
        }
        let phase = self
            .phase_resolutions
            .get(inputs.sampled_binding_phase)
            .ok_or_else(|| {
                format!(
                    "scene-owned sampled binding phase {} is missing",
                    inputs.sampled_binding_phase
                )
            })?;
        for slice in &self.slices {
            let draw = draws.get(slice.draw_index).ok_or_else(|| {
                format!("scene-owned uniform draw {} is missing", slice.draw_index)
            })?;
            let bytes = output
                .get_mut(
                    slice.byte_offset as usize
                        ..slice.byte_offset as usize + slice.byte_size as usize,
                )
                .ok_or_else(|| {
                    format!(
                        "scene-owned uniform draw {} slice exceeds arena",
                        slice.draw_index
                    )
                })?;
            bytes.fill(0);
            for member in &slice.members {
                let destination = bytes
                    .get_mut(
                        member.byte_offset as usize
                            ..member.byte_offset as usize + member.byte_size as usize,
                    )
                    .ok_or_else(|| {
                        format!(
                            "scene-owned uniform draw {} member exceeds its slice",
                            slice.draw_index
                        )
                    })?;
                match &member.source {
                    SceneOwnedRetainedSource::SceneTime => {
                        write_values(destination, &[inputs.scene_time_seconds])?;
                    }
                    SceneOwnedRetainedSource::FrameDelta => {
                        write_values(destination, &[inputs.frame_delta_seconds])?;
                    }
                    SceneOwnedRetainedSource::AudioSpectrum {
                        channel,
                        resolution,
                    } => write_audio_spectrum(
                        destination,
                        inputs.audio_spectrum,
                        *channel,
                        *resolution,
                        member.array_stride,
                    )?,
                    SceneOwnedRetainedSource::ModelViewProjectionMatrix => {
                        let matrix = scene_owned_projection_matrix(
                            draw.projection_domain,
                            draw.clip_transform,
                            self.scene_cover_clip_scale,
                        );
                        write_matrix(destination, &matrix)?;
                    }
                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix => {
                        let matrix = scene_owned_projection_matrix(
                            draw.projection_domain,
                            draw.effect_model_view_projection_matrix,
                            self.scene_cover_clip_scale,
                        );
                        write_matrix(destination, &matrix)?;
                    }
                    SceneOwnedRetainedSource::EffectTextureProjectionMatrixInverse => {
                        let matrix = apply_scene_cover_clip_scale(
                            draw.effect_texture_projection_matrix,
                            self.scene_cover_clip_scale,
                        );
                        let inverse = inverse_affine_rows(&matrix).ok_or_else(|| {
                            format!(
                                "scene-owned draw {} has a non-invertible affine effect texture projection",
                                slice.draw_index
                            )
                        })?;
                        write_matrix(destination, &inverse)?;
                    }
                    SceneOwnedRetainedSource::LayerModelMatrix => {
                        write_matrix(destination, &draw.render_world_matrix)?;
                    }
                    SceneOwnedRetainedSource::ObjectColor4 => {
                        let color = if draw.apply_resolved_visual {
                            [
                                draw.resolved_color.x,
                                draw.resolved_color.y,
                                draw.resolved_color.z,
                                draw.resolved_alpha,
                            ]
                        } else {
                            [1.0; 4]
                        };
                        write_values(destination, &color)?;
                    }
                    SceneOwnedRetainedSource::ObjectAlpha => {
                        let alpha = if draw.apply_resolved_visual {
                            draw.resolved_alpha
                        } else {
                            1.0
                        };
                        write_values(destination, &[alpha])?;
                    }
                    SceneOwnedRetainedSource::ParallaxPosition => {
                        write_values(destination, &inputs.parallax_position)?;
                    }
                    SceneOwnedRetainedSource::CurrentRenderTargetTexelSize { texel_size } => {
                        write_values(destination, texel_size)?;
                    }
                    SceneOwnedRetainedSource::SampledTextureResolution { sampled_slot_index } => {
                        let index = slice
                            .draw_index
                            .checked_mul(self.sampled_slots.len())
                            .and_then(|base| base.checked_add(*sampled_slot_index))
                            .ok_or_else(|| {
                                "scene-owned sampled resolution index overflows".to_owned()
                            })?;
                        let resolution = phase.get(index).ok_or_else(|| {
                            format!(
                                "scene-owned draw {} sampled resolution {} is missing",
                                slice.draw_index, sampled_slot_index
                            )
                        })?;
                        write_values(destination, resolution)?;
                    }
                    SceneOwnedRetainedSource::MaterialConstant {
                        constant_index,
                        default_values,
                    } => {
                        if let Some(value) = inputs.scalar_overrides.iter().find(|value| {
                            value.object == draw.object && value.constant_index == *constant_index
                        }) {
                            write_values(destination, &[value.value])?;
                        } else {
                            write_values(destination, default_values)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn current_render_target_texel_size(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    draw_index: usize,
    output_extent: [u32; 2],
) -> Result<[f32; 2], String> {
    let draw_index =
        u32::try_from(draw_index).map_err(|_| "scene-owned draw index exceeds u32".to_owned())?;
    let mut matching_passes = graph.pass_nodes.iter().filter(|pass| {
        draw_index >= pass.mesh_draw_start
            && draw_index < pass.mesh_draw_start.saturating_add(pass.mesh_draw_count)
    });
    let pass = matching_passes
        .next()
        .ok_or_else(|| format!("scene-owned draw {draw_index} has no render pass"))?;
    if matching_passes.next().is_some() {
        return Err(format!(
            "scene-owned draw {draw_index} belongs to overlapping render passes"
        ));
    }
    let extent = if pass.target == crate::engine::scene::SceneRenderTargetKind::SceneColor {
        vulkan_renderer::Extent2D::new(output_extent[0], output_extent[1])
    } else {
        let allocation = graph
            .target_allocations
            .iter()
            .find(|allocation| {
                allocation.graph_index == pass.graph_index
                    && allocation.target == pass.target
                    && allocation.target_name == pass.target_name
            })
            .copied()
            .ok_or_else(|| {
                format!(
                    "scene-owned draw {draw_index} target graph {} {:?}:{:?} has no allocation",
                    pass.graph_index, pass.target, pass.target_name
                )
            })?;
        super::effect_target::scene_logical_target_extent(
            storage,
            allocation,
            vulkan_renderer::Extent2D::new(output_extent[0], output_extent[1]),
        )?
    };
    if extent.width == 0 || extent.height == 0 {
        return Err(format!(
            "scene-owned draw {draw_index} render target has zero extent"
        ));
    }
    Ok([1.0 / extent.width as f32, 1.0 / extent.height as f32])
}

fn scene_owned_projection_matrix(
    domain: SceneRenderingDeviceProjectionDomain,
    matrix: [[f32; 4]; 4],
    scene_cover_clip_scale: [f32; 2],
) -> [[f32; 4]; 4] {
    if domain == SceneRenderingDeviceProjectionDomain::Scene {
        apply_scene_cover_clip_scale(matrix, scene_cover_clip_scale)
    } else {
        matrix
    }
}

fn material_constant_source(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    authored_name: &str,
) -> Result<(u32, Vec<f32>), String> {
    let material = storage
        .document()
        .materials
        .get(material.0 as usize)
        .ok_or_else(|| {
            format!("scene-owned parameter {authored_name:?} has no authored material")
        })?;
    let pass = storage
        .document()
        .material_passes
        .get(material.pass_start as usize)
        .ok_or_else(|| format!("scene-owned parameter {authored_name:?} has no material pass"))?;
    let mut matches = material_pass_constants(storage, pass)
        .iter()
        .enumerate()
        .filter(|(_, constant)| storage.string(constant.name) == Some(authored_name));
    let (local_index, constant) = matches
        .next()
        .ok_or_else(|| format!("scene-owned material has no exact parameter {authored_name:?}"))?;
    if matches.next().is_some() {
        return Err(format!(
            "scene-owned material has duplicate exact parameter {authored_name:?}"
        ));
    }
    let value_json = storage
        .string(constant.value_json)
        .ok_or_else(|| format!("scene-owned material parameter {authored_name:?} has no value"))?;
    let values = parse_constant_values(value_json);
    if values.is_empty() {
        return Err(format!(
            "scene-owned material parameter {authored_name:?} has no numeric default"
        ));
    }
    Ok((pass.constant_start + local_index as u32, values))
}

fn phase_resolutions(
    storage: &SceneStorage,
    draw_count: usize,
    layout: &ScenePipelineDescriptorLayout,
    phase: &SceneSampledImageBindingPlan,
    effect_targets: &[SceneEffectTargetImagePlan],
    output_extent: [u32; 2],
) -> Result<Vec<[f32; 4]>, String> {
    if phase.sampled_slot_count != layout.sampled_slots.len() {
        return Err(
            "scene-owned sampled binding width does not match descriptor layout".to_owned(),
        );
    }
    let mut resolutions = Vec::with_capacity(draw_count.saturating_mul(phase.sampled_slot_count));
    for draw_index in 0..draw_count {
        for sampled_index in 0..phase.sampled_slot_count {
            let source = phase.source(draw_index, sampled_index).ok_or_else(|| {
                format!("scene-owned draw {draw_index} sampled slot {sampled_index} has no source")
            })?;
            resolutions.push(sampled_source_resolution(
                storage,
                source,
                effect_targets,
                output_extent,
            )?);
        }
    }
    Ok(resolutions)
}

fn sampled_source_resolution(
    storage: &SceneStorage,
    source: SceneSampledImageSource,
    effect_targets: &[SceneEffectTargetImagePlan],
    output_extent: [u32; 2],
) -> Result<[f32; 4], String> {
    let dimensions = match source {
        SceneSampledImageSource::FallbackWhite => [1, 1, 1, 1],
        SceneSampledImageSource::SceneTexture { resource } => {
            let texture = storage
                .textures()
                .iter()
                .find(|texture| texture.resource == resource)
                .ok_or_else(|| {
                    format!(
                        "scene-owned sampled texture resource {} is missing",
                        resource.0
                    )
                })?;
            [
                texture.storage_width,
                texture.storage_height,
                texture.width,
                texture.height,
            ]
        }
        SceneSampledImageSource::SceneColorSnapshot => [
            output_extent[0],
            output_extent[1],
            output_extent[0],
            output_extent[1],
        ],
        SceneSampledImageSource::EffectTarget { physical_slot, .. } => {
            let target = effect_targets
                .iter()
                .find(|target| target.physical_slot == physical_slot)
                .ok_or_else(|| {
                    format!(
                        "scene-owned sampled effect target physical slot {physical_slot} is missing"
                    )
                })?;
            [
                target.extent.width,
                target.extent.height,
                target.extent.width,
                target.extent.height,
            ]
        }
        SceneSampledImageSource::VideoFramePlane {
            media_instance,
            plane,
        } => {
            return Err(format!(
                "scene-owned video media instance {media_instance} plane {plane:?} has no cold retained resolution"
            ));
        }
    };
    Ok(dimensions.map(|value| value.max(1) as f32))
}

fn align_up(value: u64, alignment: u64, role: &str) -> Result<u64, String> {
    let remainder = value % alignment;
    if remainder == 0 {
        return Ok(value);
    }
    value
        .checked_add(alignment - remainder)
        .ok_or_else(|| format!("{role} alignment overflows"))
}
