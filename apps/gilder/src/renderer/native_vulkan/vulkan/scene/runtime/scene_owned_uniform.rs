//! Retained, aligned uniform-buffer storage for scene-owned graphics programs.

use crate::engine::scene::semantic_world::ResolvedMaterialScalarValue;
use crate::engine::scene::{
    SceneMaterialHandle, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneStorage,
    StereoSpectrum64,
};
use serde::Serialize;
use vulkanalia::vk;

use super::ScenePipelineDescriptorLayout;
use super::effect_target::SceneEffectTargetImagePlan;
use super::material_uniform::{material_pass_constants, parse_constant_values};
use super::sampled_binding::{SceneSampledImageBindingPlan, SceneSampledImageSource};
use super::shader_program::{
    SceneOwnedUniformSource, SceneResolvedGraphicsProgram, resolve_scene_graphics_program,
    scene_owned_stage_resource_plan,
};

mod payload;

use payload::{write_matrix, write_strided_values, write_values};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneOwnedUniformArenaPlan {
    pub(super) byte_count: u64,
    slices: Vec<SceneOwnedUniformSlicePlan>,
    sampled_slots: Vec<u32>,
    phase_resolutions: Vec<Vec<[f32; 4]>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedUniformFrameInputs<'a> {
    pub scalar_overrides: &'a [ResolvedMaterialScalarValue],
    pub scene_time_seconds: f32,
    pub frame_delta_seconds: f32,
    pub audio_spectrum: &'a StereoSpectrum64,
    pub sampled_binding_phase: usize,
}

impl SceneOwnedUniformFrameInputs<'static> {
    pub(super) const INITIAL: Self = Self {
        scalar_overrides: &[],
        scene_time_seconds: 0.0,
        frame_delta_seconds: 0.0,
        audio_spectrum: &StereoSpectrum64::ZERO,
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
    AudioSpectrum64Left,
    AudioSpectrum64Right,
    ModelViewProjectionMatrix,
    EffectModelViewProjectionMatrix,
    LayerModelMatrix,
    SampledTextureResolution {
        sampled_slot_index: usize,
    },
    MaterialConstant {
        constant_index: u32,
        default_values: Vec<f32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneOwnedUniformArenaPlanSnapshot {
    pub output_extent: [u32; 2],
    pub min_uniform_buffer_offset_alignment: u64,
    pub byte_count: u64,
    pub slice_count: usize,
    pub sampled_binding_phase_count: usize,
    pub initial_payload_byte_count: usize,
    pub descriptor_slices: Vec<NativeVulkanSceneOwnedUniformSliceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneOwnedUniformSliceSnapshot {
    pub draw_index: usize,
    pub descriptor_lane: usize,
    pub byte_offset: u64,
    pub byte_size: u64,
}

pub fn native_vulkan_scene_owned_uniform_arena_plan(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    output_extent: [u32; 2],
    min_uniform_buffer_offset_alignment: u64,
) -> Result<NativeVulkanSceneOwnedUniformArenaPlanSnapshot, String> {
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
        vk::Format::B8G8R8A8_SRGB,
        vk::Extent2D {
            width: output_extent[0],
            height: output_extent[1],
        },
    )?;
    let plan = SceneOwnedUniformArenaPlan::build(
        storage,
        &graph.mesh_draws,
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
            NativeVulkanSceneOwnedUniformSliceSnapshot {
                draw_index,
                descriptor_lane,
                byte_offset,
                byte_size,
            }
        })
        .collect::<Vec<_>>();
    Ok(NativeVulkanSceneOwnedUniformArenaPlanSnapshot {
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
    pub(super) fn build(
        storage: &SceneStorage,
        draws: &[SceneRenderingDeviceMeshDraw],
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
        for (draw_index, draw) in draws.iter().enumerate() {
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
                                SceneOwnedUniformSource::AudioSpectrum64Left => {
                                    SceneOwnedRetainedSource::AudioSpectrum64Left
                                }
                                SceneOwnedUniformSource::AudioSpectrum64Right => {
                                    SceneOwnedRetainedSource::AudioSpectrum64Right
                                }
                                SceneOwnedUniformSource::ModelViewProjectionMatrix => {
                                    SceneOwnedRetainedSource::ModelViewProjectionMatrix
                                }
                                SceneOwnedUniformSource::EffectModelViewProjectionMatrix => {
                                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix
                                }
                                SceneOwnedUniformSource::LayerModelMatrix => {
                                    SceneOwnedRetainedSource::LayerModelMatrix
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
            });
        }
        let phase_resolutions = sampled_binding_cycle
            .iter()
            .map(|phase| {
                phase_resolutions(
                    storage,
                    draws.len(),
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
                    SceneOwnedRetainedSource::AudioSpectrum64Left => {
                        write_strided_values(
                            destination,
                            &inputs.audio_spectrum.left,
                            member.array_stride,
                        )?;
                    }
                    SceneOwnedRetainedSource::AudioSpectrum64Right => {
                        write_strided_values(
                            destination,
                            &inputs.audio_spectrum.right,
                            member.array_stride,
                        )?;
                    }
                    SceneOwnedRetainedSource::ModelViewProjectionMatrix => {
                        write_matrix(destination, &draw.clip_transform)?;
                    }
                    SceneOwnedRetainedSource::EffectModelViewProjectionMatrix => {
                        write_matrix(destination, &draw.effect_model_view_projection_matrix)?;
                    }
                    SceneOwnedRetainedSource::LayerModelMatrix => {
                        write_matrix(destination, &draw.render_world_matrix)?;
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
            [target.width, target.height, target.width, target.height]
        }
        SceneSampledImageSource::VideoFrame { media_instance } => {
            return Err(format!(
                "scene-owned video media instance {media_instance} has no retained resolution"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneObjectHandle, SceneRenderingDeviceDrawPrimitive, SceneStringId,
    };

    #[test]
    fn alignment_uses_the_device_limit_without_power_of_two_assumptions() {
        assert_eq!(align_up(0, 192, "test").unwrap(), 0);
        assert_eq!(align_up(176, 192, "test").unwrap(), 192);
        assert_eq!(align_up(240, 192, "test").unwrap(), 384);
    }

    #[test]
    fn retained_payload_updates_matrices_resolution_and_scalar_without_allocating_sources() {
        let plan = SceneOwnedUniformArenaPlan {
            byte_count: 320,
            slices: vec![SceneOwnedUniformSlicePlan {
                draw_index: 0,
                descriptor_lane: 0,
                byte_offset: 0,
                byte_size: 220,
                members: vec![
                    member(0, 64, SceneOwnedRetainedSource::ModelViewProjectionMatrix),
                    member(
                        64,
                        64,
                        SceneOwnedRetainedSource::EffectModelViewProjectionMatrix,
                    ),
                    member(128, 64, SceneOwnedRetainedSource::LayerModelMatrix),
                    member(
                        192,
                        16,
                        SceneOwnedRetainedSource::SampledTextureResolution {
                            sampled_slot_index: 0,
                        },
                    ),
                    member(
                        208,
                        4,
                        SceneOwnedRetainedSource::MaterialConstant {
                            constant_index: 9,
                            default_values: vec![0.25],
                        },
                    ),
                    member(212, 4, SceneOwnedRetainedSource::SceneTime),
                    member(216, 4, SceneOwnedRetainedSource::FrameDelta),
                ],
            }],
            sampled_slots: vec![0],
            phase_resolutions: vec![vec![[128.0, 64.0, 100.0, 50.0]]],
        };
        let mut draw = draw();
        draw.clip_transform[0][0] = 2.0;
        draw.effect_model_view_projection_matrix[0][0] = 9.0;
        draw.render_world_matrix[3][0] = 17.0;
        let override_value = ResolvedMaterialScalarValue {
            object: draw.object,
            constant_index: 9,
            value: 0.75,
        };
        let mut payload = vec![0xcc; 320];

        plan.write_payload(
            &[draw],
            SceneOwnedUniformFrameInputs {
                scalar_overrides: &[override_value],
                scene_time_seconds: 1.25,
                frame_delta_seconds: 0.5,
                audio_spectrum: &StereoSpectrum64::ZERO,
                sampled_binding_phase: 0,
            },
            &mut payload,
        )
        .expect("retained payload");

        assert_eq!(read_f32(&payload, 0), 2.0);
        assert_eq!(read_f32(&payload, 64), 9.0);
        assert_eq!(read_f32(&payload, 176), 17.0);
        assert_eq!(read_f32(&payload, 192), 128.0);
        assert_eq!(read_f32(&payload, 204), 50.0);
        assert_eq!(read_f32(&payload, 208), 0.75);
        assert_eq!(read_f32(&payload, 212), 1.25);
        assert_eq!(read_f32(&payload, 216), 0.5);
        assert!(payload[220..].iter().all(|byte| *byte == 0xcc));
    }

    #[test]
    fn retained_payload_preserves_strided_stereo64_channels() {
        let array = |byte_offset, source| SceneOwnedUniformMemberSource {
            byte_offset,
            byte_size: 1012,
            array_stride: 16,
            source,
        };
        let plan = SceneOwnedUniformArenaPlan {
            byte_count: 2048,
            slices: vec![SceneOwnedUniformSlicePlan {
                draw_index: 0,
                descriptor_lane: 0,
                byte_offset: 0,
                byte_size: 2036,
                members: vec![
                    array(0, SceneOwnedRetainedSource::AudioSpectrum64Left),
                    array(1024, SceneOwnedRetainedSource::AudioSpectrum64Right),
                ],
            }],
            sampled_slots: Vec::new(),
            phase_resolutions: vec![Vec::new()],
        };
        let spectrum = StereoSpectrum64 {
            left: std::array::from_fn(|index| index as f32),
            right: std::array::from_fn(|index| 100.0 + index as f32),
        };
        let mut payload = vec![0xcc; 2048];

        plan.write_payload(
            &[draw()],
            SceneOwnedUniformFrameInputs {
                scalar_overrides: &[],
                scene_time_seconds: 0.0,
                frame_delta_seconds: 0.0,
                audio_spectrum: &spectrum,
                sampled_binding_phase: 0,
            },
            &mut payload,
        )
        .expect("stereo64 payload");

        assert_eq!(read_f32(&payload, 16), 1.0);
        assert_eq!(read_f32(&payload, 1008), 63.0);
        assert_eq!(read_f32(&payload, 1024), 100.0);
        assert_eq!(read_f32(&payload, 2032), 163.0);
        assert!(payload[4..16].iter().all(|byte| *byte == 0));
        assert!(payload[2036..].iter().all(|byte| *byte == 0xcc));
    }

    fn member(
        byte_offset: u32,
        byte_size: u32,
        source: SceneOwnedRetainedSource,
    ) -> SceneOwnedUniformMemberSource {
        SceneOwnedUniformMemberSource {
            byte_offset,
            byte_size,
            array_stride: 0,
            source,
        }
    }

    fn draw() -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            shader_key: SceneStringId::NONE,
            mesh_index: 0,
            resolved_object_index: 0,
            render_world_matrix: [[0.0; 4]; 4],
            clip_transform: [[0.0; 4]; 4],
            effect_model_view_projection_matrix: [[0.0; 4]; 4],
            authored_source_extent: [1.0; 2],
            skinning_palette_start: 0,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3::default(),
            resolved_alpha: 1.0,
            apply_resolved_visual: false,
            effect_batch_atlas_tile: u32::MAX,
            effect_batch_atlas_grid: [0; 2],
            effect_binding_start: 0,
            effect_binding_count: 0,
            effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            resolved_effect_visibility_mask: 0,
            object: SceneObjectHandle(3),
            material: SceneMaterialHandle(0),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            instance_count: 1,
        }
    }

    fn read_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
