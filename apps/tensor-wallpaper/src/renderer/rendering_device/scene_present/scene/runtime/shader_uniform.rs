//! Allocation-free payload writes for scene-owned typed uniform buffers.

use super::shader_program::{
    SceneOwnedUniformBufferPlan, SceneOwnedUniformMemberPlan, SceneOwnedUniformSource,
};
use crate::engine::scene::{SceneRenderingDeviceMeshDraw, StereoSpectrum64};

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedMaterialParameterValue<'a> {
    pub authored_name: &'a str,
    pub values: &'a [f32],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedUniformPayloadInputs<'a> {
    pub scene_time_seconds: f32,
    pub frame_delta_seconds: f32,
    pub audio_spectrum: &'a StereoSpectrum64,
    pub model_view_projection_matrix: &'a [[f32; 4]; 4],
    pub effect_model_view_projection_matrix: &'a [[f32; 4]; 4],
    pub effect_texture_projection_matrix: &'a [[f32; 4]; 4],
    pub layer_model_matrix: &'a [[f32; 4]; 4],
    pub object_color4: [f32; 4],
    pub object_alpha: f32,
    pub parallax_position: [f32; 2],
    pub current_render_target_texel_size: [f32; 2],
    pub sampled_texture_resolutions: &'a [(u32, [f32; 4])],
    pub material_parameters: &'a [SceneOwnedMaterialParameterValue<'a>],
}

impl<'a> SceneOwnedUniformPayloadInputs<'a> {
    pub(super) fn for_draw(
        draw: &'a SceneRenderingDeviceMeshDraw,
        sampled_texture_resolutions: &'a [(u32, [f32; 4])],
        material_parameters: &'a [SceneOwnedMaterialParameterValue<'a>],
    ) -> Self {
        let (object_color4, object_alpha) = if draw.apply_resolved_visual {
            (
                [
                    draw.resolved_color.x,
                    draw.resolved_color.y,
                    draw.resolved_color.z,
                    draw.resolved_alpha,
                ],
                draw.resolved_alpha,
            )
        } else {
            ([1.0; 4], 1.0)
        };
        Self {
            scene_time_seconds: 0.0,
            frame_delta_seconds: 0.0,
            audio_spectrum: &StereoSpectrum64::ZERO,
            model_view_projection_matrix: &draw.clip_transform,
            effect_model_view_projection_matrix: &draw.effect_model_view_projection_matrix,
            effect_texture_projection_matrix: &draw.effect_texture_projection_matrix,
            layer_model_matrix: &draw.render_world_matrix,
            object_color4,
            object_alpha,
            parallax_position: [0.5; 2],
            current_render_target_texel_size: [1.0; 2],
            sampled_texture_resolutions,
            material_parameters,
        }
    }
}

pub(super) fn write_scene_owned_uniform_payload(
    plan: &SceneOwnedUniformBufferPlan<'_>,
    inputs: SceneOwnedUniformPayloadInputs<'_>,
    output: &mut [u8],
) -> Result<(), String> {
    if output.len() != plan.byte_size as usize {
        return Err(format!(
            "scene-owned uniform buffer {:?} requires {} bytes, received {}",
            plan.name,
            plan.byte_size,
            output.len()
        ));
    }
    output.fill(0);
    for member in &plan.members {
        let destination = member_bytes(output, plan, member)?;
        match member.source {
            SceneOwnedUniformSource::SceneTime => {
                write_f32_values(destination, &[inputs.scene_time_seconds])?;
            }
            SceneOwnedUniformSource::FrameDelta => {
                write_f32_values(destination, &[inputs.frame_delta_seconds])?;
            }
            SceneOwnedUniformSource::AudioSpectrum {
                channel,
                resolution,
            } => write_audio_spectrum(
                destination,
                inputs.audio_spectrum,
                channel,
                resolution,
                member.array_stride,
            )?,
            SceneOwnedUniformSource::ModelViewProjectionMatrix => {
                write_matrix(destination, inputs.model_view_projection_matrix)?;
            }
            SceneOwnedUniformSource::EffectModelViewProjectionMatrix => {
                write_matrix(destination, inputs.effect_model_view_projection_matrix)?;
            }
            SceneOwnedUniformSource::EffectTextureProjectionMatrixInverse => {
                let inverse = inverse_affine_rows(inputs.effect_texture_projection_matrix)
                    .ok_or_else(|| {
                        format!(
                            "scene-owned uniform {:?} requires an invertible affine effect texture projection",
                            member.name
                        )
                    })?;
                write_matrix(destination, &inverse)?;
            }
            SceneOwnedUniformSource::LayerModelMatrix => {
                write_matrix(destination, inputs.layer_model_matrix)?;
            }
            SceneOwnedUniformSource::ObjectColor4 => {
                write_f32_values(destination, &inputs.object_color4)?;
            }
            SceneOwnedUniformSource::ObjectAlpha => {
                write_f32_values(destination, &[inputs.object_alpha])?;
            }
            SceneOwnedUniformSource::ParallaxPosition => {
                write_f32_values(destination, &inputs.parallax_position)?;
            }
            SceneOwnedUniformSource::CurrentRenderTargetTexelSize => {
                write_f32_values(destination, &inputs.current_render_target_texel_size)?;
            }
            SceneOwnedUniformSource::SampledTextureResolution { slot } => {
                let resolution = inputs
                    .sampled_texture_resolutions
                    .iter()
                    .find_map(|(candidate, resolution)| (*candidate == slot).then_some(resolution))
                    .ok_or_else(|| {
                        format!(
                            "scene-owned uniform {:?} requires sampled texture slot {slot} resolution",
                            member.name
                        )
                    })?;
                write_f32_values(destination, resolution)?;
            }
            SceneOwnedUniformSource::MaterialParameter { authored_name } => {
                let values = inputs
                    .material_parameters
                    .iter()
                    .find(|parameter| parameter.authored_name == authored_name)
                    .map(|parameter| parameter.values)
                    .ok_or_else(|| {
                        format!(
                            "scene-owned uniform {:?} requires authored material parameter {authored_name:?}",
                            member.name
                        )
                    })?;
                write_f32_values(destination, values)?;
            }
        }
    }
    Ok(())
}

fn member_bytes<'a>(
    output: &'a mut [u8],
    plan: &SceneOwnedUniformBufferPlan<'_>,
    member: &SceneOwnedUniformMemberPlan<'_>,
) -> Result<&'a mut [u8], String> {
    let start = member.byte_offset as usize;
    let end = start
        .checked_add(member.byte_size as usize)
        .ok_or_else(|| format!("scene-owned uniform {:?} byte range overflows", member.name))?;
    output.get_mut(start..end).ok_or_else(|| {
        format!(
            "scene-owned uniform {:?} exceeds buffer {:?}",
            member.name, plan.name
        )
    })
}

fn write_matrix(destination: &mut [u8], matrix: &[[f32; 4]; 4]) -> Result<(), String> {
    if destination.len() != 16 * size_of::<f32>() {
        return Err(format!(
            "scene-owned matrix uniform requires {} bytes, received {}",
            16 * size_of::<f32>(),
            destination.len()
        ));
    }
    for (bytes, value) in destination.chunks_exact_mut(4).zip(matrix.iter().flatten()) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

pub(super) fn inverse_affine_rows(matrix: &[[f32; 4]; 4]) -> Option<[[f32; 4]; 4]> {
    if matrix[3] != [0.0, 0.0, 0.0, 1.0] {
        return None;
    }
    let [a00, a01, a02, tx] = matrix[0].map(f64::from);
    let [a10, a11, a12, ty] = matrix[1].map(f64::from);
    let [a20, a21, a22, tz] = matrix[2].map(f64::from);
    let determinant = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let reciprocal = determinant.recip();
    let i00 = (a11 * a22 - a12 * a21) * reciprocal;
    let i01 = (a02 * a21 - a01 * a22) * reciprocal;
    let i02 = (a01 * a12 - a02 * a11) * reciprocal;
    let i10 = (a12 * a20 - a10 * a22) * reciprocal;
    let i11 = (a00 * a22 - a02 * a20) * reciprocal;
    let i12 = (a02 * a10 - a00 * a12) * reciprocal;
    let i20 = (a10 * a21 - a11 * a20) * reciprocal;
    let i21 = (a01 * a20 - a00 * a21) * reciprocal;
    let i22 = (a00 * a11 - a01 * a10) * reciprocal;
    let inverse_f64 = [
        [i00, i01, i02, -(i00 * tx + i01 * ty + i02 * tz)],
        [i10, i11, i12, -(i10 * tx + i11 * ty + i12 * tz)],
        [i20, i21, i22, -(i20 * tx + i21 * ty + i22 * tz)],
        [0.0, 0.0, 0.0, 1.0],
    ];
    if !inverse_f64.iter().flatten().all(|value| value.is_finite()) {
        return None;
    }
    let inverse = inverse_f64.map(|row| row.map(|value| value as f32));
    inverse
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then_some(inverse)
}

fn write_f32_values(destination: &mut [u8], values: &[f32]) -> Result<(), String> {
    let expected = values
        .len()
        .checked_mul(size_of::<f32>())
        .ok_or_else(|| "scene-owned uniform value byte count overflows".to_owned())?;
    if destination.len() != expected {
        return Err(format!(
            "scene-owned uniform member requires {} bytes, received {expected}",
            destination.len()
        ));
    }
    for (bytes, value) in destination.chunks_exact_mut(4).zip(values) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_strided_f32_values(
    destination: &mut [u8],
    values: &[f32],
    array_stride: u32,
) -> Result<(), String> {
    let stride = array_stride as usize;
    let expected = values
        .len()
        .checked_mul(stride)
        .ok_or_else(|| "scene-owned uniform array byte count overflows".to_owned())?;
    if stride < size_of::<f32>() || destination.len() != expected {
        return Err(format!(
            "scene-owned uniform array requires {expected} bytes at stride {stride}, received {}",
            destination.len()
        ));
    }
    for (index, value) in values.iter().enumerate() {
        let offset = index * stride;
        destination[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_audio_spectrum(
    destination: &mut [u8],
    spectrum: &StereoSpectrum64,
    channel: super::shader_program::SceneAudioSpectrumChannel,
    resolution: super::shader_program::SceneAudioSpectrumResolution,
    array_stride: u32,
) -> Result<(), String> {
    use super::shader_program::SceneAudioSpectrumChannel::{Left, Right};
    use super::shader_program::SceneAudioSpectrumResolution::{Bands16, Bands32, Bands64};

    let channel64 = match channel {
        Left => &spectrum.left,
        Right => &spectrum.right,
    };
    match resolution {
        Bands64 => write_strided_f32_values(destination, channel64, array_stride),
        Bands32 => {
            let channel32 = StereoSpectrum64::max_pool_32(channel64);
            write_strided_f32_values(destination, &channel32, array_stride)
        }
        Bands16 => {
            let channel32 = StereoSpectrum64::max_pool_32(channel64);
            let channel16 = StereoSpectrum64::max_pool_16(&channel32);
            write_strided_f32_values(destination, &channel16, array_stride)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneShaderScalarType, SceneShaderUniformMemberRecord};

    #[test]
    fn rounded_mask_payload_writes_typed_offsets_without_catalog_layout() {
        let members = vec![
            member(
                "g_ModelViewProjectionMatrix",
                SceneOwnedUniformSource::ModelViewProjectionMatrix,
                0,
                64,
            ),
            member(
                "g_LayerModelMatrix",
                SceneOwnedUniformSource::LayerModelMatrix,
                64,
                64,
            ),
            member("g_Time", SceneOwnedUniformSource::SceneTime, 128, 4),
            member("g_FrameTime", SceneOwnedUniformSource::FrameDelta, 132, 4),
        ];
        let plan = SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 176,
            members,
        };
        let projection = matrix(1.0);
        let layer = matrix(17.0);
        let mut payload = vec![0xcc; 176];

        write_scene_owned_uniform_payload(
            &plan,
            SceneOwnedUniformPayloadInputs {
                scene_time_seconds: 1.25,
                frame_delta_seconds: 0.5,
                audio_spectrum: &StereoSpectrum64::ZERO,
                model_view_projection_matrix: &projection,
                effect_model_view_projection_matrix: &projection,
                effect_texture_projection_matrix: &projection,
                layer_model_matrix: &layer,
                object_color4: [1.0; 4],
                object_alpha: 1.0,
                parallax_position: [0.5; 2],
                current_render_target_texel_size: [1.0; 2],
                sampled_texture_resolutions: &[],
                material_parameters: &[],
            },
            &mut payload,
        )
        .expect("vertex payload");

        assert_eq!(read_f32(&payload, 0), 1.0);
        assert_eq!(read_f32(&payload, 60), 16.0);
        assert_eq!(read_f32(&payload, 64), 17.0);
        assert_eq!(read_f32(&payload, 124), 32.0);
        assert_eq!(read_f32(&payload, 128), 1.25);
        assert_eq!(read_f32(&payload, 132), 0.5);
        assert!(payload[136..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn rounded_mask_fragment_payload_uses_texture_and_authored_values() {
        let members = vec![
            member(
                "u_Color",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Color",
                },
                0,
                12,
            ),
            member(
                "g_Texture0Resolution",
                SceneOwnedUniformSource::SampledTextureResolution { slot: 0 },
                16,
                16,
            ),
            member(
                "u_Radius",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Radius",
                },
                32,
                4,
            ),
            member(
                "u_BorderWidth",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Border width",
                },
                36,
                4,
            ),
            member(
                "u_Softness",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Softness",
                },
                40,
                4,
            ),
            member(
                "u_Alpha",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "ui_editor_properties_opacity",
                },
                44,
                4,
            ),
        ];
        let plan = SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 48,
            members,
        };
        let color = [0.1, 0.2, 0.3];
        let radius = [12.0];
        let border = [2.0];
        let softness = [0.5];
        let alpha = [0.75];
        let material_parameters = [
            parameter("Color", &color),
            parameter("Radius", &radius),
            parameter("Border width", &border),
            parameter("Softness", &softness),
            parameter("ui_editor_properties_opacity", &alpha),
        ];
        let identity = matrix(0.0);
        let mut payload = [0u8; 48];

        write_scene_owned_uniform_payload(
            &plan,
            SceneOwnedUniformPayloadInputs {
                scene_time_seconds: 0.0,
                frame_delta_seconds: 0.0,
                audio_spectrum: &StereoSpectrum64::ZERO,
                model_view_projection_matrix: &identity,
                effect_model_view_projection_matrix: &identity,
                effect_texture_projection_matrix: &identity,
                layer_model_matrix: &identity,
                object_color4: [1.0; 4],
                object_alpha: 1.0,
                parallax_position: [0.5; 2],
                current_render_target_texel_size: [1.0; 2],
                sampled_texture_resolutions: &[(0, [1920.0, 1080.0, 1920.0, 1080.0])],
                material_parameters: &material_parameters,
            },
            &mut payload,
        )
        .expect("fragment payload");

        assert_eq!(read_f32(&payload, 0), 0.1);
        assert_eq!(read_f32(&payload, 8), 0.3);
        assert_eq!(&payload[12..16], &[0; 4]);
        assert_eq!(read_f32(&payload, 16), 1920.0);
        assert_eq!(read_f32(&payload, 20), 1080.0);
        assert_eq!(read_f32(&payload, 32), 12.0);
        assert_eq!(read_f32(&payload, 36), 2.0);
        assert_eq!(read_f32(&payload, 40), 0.5);
        assert_eq!(read_f32(&payload, 44), 0.75);
    }

    #[test]
    fn missing_typed_source_fails_instead_of_zero_filling() {
        let plan = SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 4,
            members: vec![member(
                "u_Radius",
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Radius",
                },
                0,
                4,
            )],
        };
        let identity = matrix(0.0);
        let mut payload = [0u8; 4];

        let error = write_scene_owned_uniform_payload(
            &plan,
            SceneOwnedUniformPayloadInputs {
                scene_time_seconds: 0.0,
                frame_delta_seconds: 0.0,
                audio_spectrum: &StereoSpectrum64::ZERO,
                model_view_projection_matrix: &identity,
                effect_model_view_projection_matrix: &identity,
                effect_texture_projection_matrix: &identity,
                layer_model_matrix: &identity,
                object_color4: [1.0; 4],
                object_alpha: 1.0,
                parallax_position: [0.5; 2],
                current_render_target_texel_size: [1.0; 2],
                sampled_texture_resolutions: &[],
                material_parameters: &[],
            },
            &mut payload,
        )
        .expect_err("missing authored value must fail");

        assert!(error.contains("requires authored material parameter"));
    }

    #[test]
    fn effect_mvp_uses_its_independent_typed_matrix() {
        let plan = SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 64,
            members: vec![member(
                "g_EffectModelViewProjectionMatrix",
                SceneOwnedUniformSource::EffectModelViewProjectionMatrix,
                0,
                64,
            )],
        };
        let ordinary = matrix(1.0);
        let effect = matrix(33.0);
        let mut payload = [0u8; 64];

        write_scene_owned_uniform_payload(
            &plan,
            SceneOwnedUniformPayloadInputs {
                scene_time_seconds: 0.0,
                frame_delta_seconds: 0.0,
                audio_spectrum: &StereoSpectrum64::ZERO,
                model_view_projection_matrix: &ordinary,
                effect_model_view_projection_matrix: &effect,
                effect_texture_projection_matrix: &ordinary,
                layer_model_matrix: &ordinary,
                object_color4: [1.0; 4],
                object_alpha: 1.0,
                parallax_position: [0.5; 2],
                current_render_target_texel_size: [1.0; 2],
                sampled_texture_resolutions: &[],
                material_parameters: &[],
            },
            &mut payload,
        )
        .expect("effect MVP payload");

        assert_eq!(read_f32(&payload, 0), 33.0);
        assert_eq!(read_f32(&payload, 60), 48.0);
    }

    #[test]
    fn direct_payload_writes_current_render_target_texel_size() {
        let plan = SceneOwnedUniformBufferPlan {
            name: "GlobalParams",
            register: 0,
            byte_size: 8,
            members: vec![member(
                "g_TexelSize",
                SceneOwnedUniformSource::CurrentRenderTargetTexelSize,
                0,
                8,
            )],
        };
        let identity = matrix(0.0);
        let inputs = SceneOwnedUniformPayloadInputs {
            scene_time_seconds: 0.0,
            frame_delta_seconds: 0.0,
            audio_spectrum: &StereoSpectrum64::ZERO,
            model_view_projection_matrix: &identity,
            effect_model_view_projection_matrix: &identity,
            effect_texture_projection_matrix: &identity,
            layer_model_matrix: &identity,
            object_color4: [1.0; 4],
            object_alpha: 1.0,
            parallax_position: [0.5; 2],
            current_render_target_texel_size: [1.0 / 3840.0, 1.0 / 2160.0],
            sampled_texture_resolutions: &[],
            material_parameters: &[],
        };
        let mut payload = [0u8; 8];

        write_scene_owned_uniform_payload(&plan, inputs, &mut payload)
            .expect("direct current render-target texel-size payload");

        assert_eq!(
            f32::from_le_bytes(payload[0..4].try_into().unwrap()),
            1.0 / 3840.0
        );
        assert_eq!(
            f32::from_le_bytes(payload[4..8].try_into().unwrap()),
            1.0 / 2160.0
        );
    }

    #[test]
    fn affine_row_inverse_cancels_scale_rotation_and_translation() {
        let matrix = [
            [0.0, -2.0, 0.0, 8.0],
            [3.0, 0.0, 0.0, -6.0],
            [0.0, 0.0, 4.0, 12.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let inverse = inverse_affine_rows(&matrix).expect("invertible affine matrix");
        let mut product = [[0.0; 4]; 4];
        for (row, product_row) in product.iter_mut().enumerate() {
            for (column, value) in product_row.iter_mut().enumerate() {
                *value = (0..4)
                    .map(|inner| matrix[row][inner] * inverse[inner][column])
                    .sum();
            }
        }
        assert_eq!(
            product,
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        );
    }

    #[test]
    fn affine_row_inverse_accepts_small_but_well_conditioned_clip_scales() {
        let matrix = [
            [2.0 / 2_880.0, 0.0, 0.0, 0.0],
            [0.0, -2.0 / 1_620.0, 0.0, 0.178_918_93],
            [0.0, 0.0, 1.0 / 3_000.0, 0.375],
            [0.0, 0.0, 0.0, 1.0],
        ];

        let inverse = inverse_affine_rows(&matrix).expect("invertible scene clip matrix");

        assert!((inverse[0][0] - 1_440.0).abs() < 0.001);
        assert!((inverse[1][1] + 810.0).abs() < 0.001);
        assert!((inverse[2][2] - 3_000.0).abs() < 0.001);
    }

    #[test]
    fn affine_row_inverse_still_rejects_a_singular_projection() {
        let matrix = [
            [0.001, 0.0, 0.0, 0.0],
            [0.001, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.001, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        assert!(inverse_affine_rows(&matrix).is_none());
    }

    fn member<'a>(
        name: &'a str,
        source: SceneOwnedUniformSource<'a>,
        byte_offset: u32,
        byte_size: u32,
    ) -> SceneOwnedUniformMemberPlan<'a> {
        let record = SceneShaderUniformMemberRecord {
            name: crate::engine::scene::SceneStringId::NONE,
            material_parameter: crate::engine::scene::SceneStringId::NONE,
            byte_offset,
            byte_size,
            scalar_type: SceneShaderScalarType::F32,
            rows: byte_size / 4,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        };
        SceneOwnedUniformMemberPlan {
            name,
            source,
            byte_offset: record.byte_offset,
            byte_size: record.byte_size,
            scalar_type: record.scalar_type,
            rows: record.rows,
            columns: record.columns,
            array_count: record.array_count,
            array_stride: record.array_stride,
            matrix_stride: record.matrix_stride,
        }
    }

    fn parameter<'a>(
        authored_name: &'a str,
        values: &'a [f32],
    ) -> SceneOwnedMaterialParameterValue<'a> {
        SceneOwnedMaterialParameterValue {
            authored_name,
            values,
        }
    }

    fn matrix(start: f32) -> [[f32; 4]; 4] {
        let mut value = start;
        std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                let current = value;
                value += 1.0;
                current
            })
        })
    }

    fn read_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("float bytes"))
    }
}
