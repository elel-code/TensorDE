//! Allocation-free payload writes for scene-owned typed uniform buffers.

use super::shader_program::{
    SceneOwnedUniformBufferPlan, SceneOwnedUniformMemberPlan, SceneOwnedUniformSource,
};
use crate::engine::scene::SceneRenderingDeviceMeshDraw;

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedMaterialParameterValue<'a> {
    pub authored_name: &'a str,
    pub values: &'a [f32],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneOwnedUniformPayloadInputs<'a> {
    pub model_view_projection_matrix: &'a [[f32; 4]; 4],
    pub layer_model_matrix: &'a [[f32; 4]; 4],
    pub sampled_texture_resolutions: &'a [(u32, [f32; 4])],
    pub material_parameters: &'a [SceneOwnedMaterialParameterValue<'a>],
}

impl<'a> SceneOwnedUniformPayloadInputs<'a> {
    pub(super) fn for_draw(
        draw: &'a SceneRenderingDeviceMeshDraw,
        sampled_texture_resolutions: &'a [(u32, [f32; 4])],
        material_parameters: &'a [SceneOwnedMaterialParameterValue<'a>],
    ) -> Self {
        Self {
            model_view_projection_matrix: &draw.clip_transform,
            layer_model_matrix: &draw.render_world_matrix,
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
            SceneOwnedUniformSource::ModelViewProjectionMatrix => {
                write_matrix(destination, inputs.model_view_projection_matrix)?;
            }
            SceneOwnedUniformSource::LayerModelMatrix => {
                write_matrix(destination, inputs.layer_model_matrix)?;
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
                    .find(|parameter| parameter.authored_name.eq_ignore_ascii_case(authored_name))
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
                model_view_projection_matrix: &projection,
                layer_model_matrix: &layer,
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
        assert!(payload[128..].iter().all(|byte| *byte == 0));
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
                model_view_projection_matrix: &identity,
                layer_model_matrix: &identity,
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
                model_view_projection_matrix: &identity,
                layer_model_matrix: &identity,
                sampled_texture_resolutions: &[],
                material_parameters: &[],
            },
            &mut payload,
        )
        .expect_err("missing authored value must fail");

        assert!(error.contains("requires authored material parameter"));
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
