//! Strict typed sources for package-owned shader uniforms.

use super::SceneOwnedUniformSource;
use crate::engine::scene::{SceneShaderScalarType, SceneShaderUniformMemberRecord};

pub(super) fn scene_owned_uniform_source<'a>(
    key: &str,
    name: &'a str,
    material_parameter: Option<&'a str>,
    member: &SceneShaderUniformMemberRecord,
) -> Result<SceneOwnedUniformSource<'a>, String> {
    if let Some(authored_name) = material_parameter {
        return Ok(SceneOwnedUniformSource::MaterialParameter { authored_name });
    }
    match name {
        "g_Time" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 1, 1, 4)?;
            Ok(SceneOwnedUniformSource::SceneTime)
        }
        "g_FrameTime" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 1, 1, 4)?;
            Ok(SceneOwnedUniformSource::FrameDelta)
        }
        "g_AudioSpectrum64Left" | "g_AudioSpectrum64Right" => {
            require_uniform_array_shape(
                key,
                name,
                member,
                SceneShaderScalarType::F32,
                64,
                16,
                1012,
            )?;
            Ok(if name.ends_with("Left") {
                SceneOwnedUniformSource::AudioSpectrum64Left
            } else {
                SceneOwnedUniformSource::AudioSpectrum64Right
            })
        }
        "g_ModelViewProjectionMatrix" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::ModelViewProjectionMatrix)
        }
        "g_LayerModelMatrix" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::LayerModelMatrix)
        }
        _ if name.starts_with("g_Texture") && name.ends_with("Resolution") => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 1, 16)?;
            let slot = name["g_Texture".len()..name.len() - "Resolution".len()]
                .parse::<u32>()
                .map_err(|_| {
                    format!(
                        "scene-owned uniform {name:?} in {key:?} has an invalid texture slot"
                    )
                })?;
            Ok(SceneOwnedUniformSource::SampledTextureResolution { slot })
        }
        _ => Err(format!(
            "scene-owned uniform {name:?} in {key:?} has no typed runtime source \
             (type={:?}, rows={}, columns={}, bytes={}, array_count={}, array_stride={}, matrix_stride={})",
            member.scalar_type,
            member.rows,
            member.columns,
            member.byte_size,
            member.array_count,
            member.array_stride,
            member.matrix_stride
        )),
    }
}

fn require_uniform_shape(
    key: &str,
    name: &str,
    member: &SceneShaderUniformMemberRecord,
    scalar_type: SceneShaderScalarType,
    rows: u32,
    columns: u32,
    byte_size: u32,
) -> Result<(), String> {
    if member.scalar_type != scalar_type
        || member.rows != rows
        || member.columns != columns
        || member.byte_size != byte_size
        || member.array_count != 1
    {
        return Err(format!(
            "scene-owned uniform {name:?} in {key:?} has an incompatible runtime shape"
        ));
    }
    Ok(())
}

fn require_uniform_array_shape(
    key: &str,
    name: &str,
    member: &SceneShaderUniformMemberRecord,
    scalar_type: SceneShaderScalarType,
    array_count: u32,
    array_stride: u32,
    byte_size: u32,
) -> Result<(), String> {
    if member.scalar_type != scalar_type
        || member.rows != 1
        || member.columns != 1
        || member.byte_size != byte_size
        || member.array_count != array_count
        || member.array_stride != array_stride
    {
        return Err(format!(
            "scene-owned uniform {name:?} in {key:?} has an incompatible runtime array shape"
        ));
    }
    Ok(())
}
