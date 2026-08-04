//! Strict typed sources for package-owned shader uniforms.

use crate::engine::scene::{SceneShaderScalarType, SceneShaderUniformMemberRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum SceneAudioSpectrumChannel {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum SceneAudioSpectrumResolution {
    Bands16,
    Bands32,
    Bands64,
}

impl SceneAudioSpectrumResolution {
    pub(in super::super) const fn band_count(self) -> u32 {
        match self {
            Self::Bands16 => 16,
            Self::Bands32 => 32,
            Self::Bands64 => 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum SceneOwnedUniformSource<'a> {
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
    CurrentRenderTargetTexelSize,
    SampledTextureResolution {
        slot: u32,
    },
    MaterialParameter {
        authored_name: &'a str,
    },
}

pub(in super::super) fn is_scene_audio_spectrum_uniform_name(name: &str) -> bool {
    scene_audio_spectrum_source(name).is_some()
}

pub(super) fn scene_owned_uniform_source<'a>(
    key: &str,
    name: &'a str,
    material_parameter: Option<&'a str>,
    member: &SceneShaderUniformMemberRecord,
) -> Result<SceneOwnedUniformSource<'a>, String> {
    if let Some(authored_name) = material_parameter {
        return Ok(SceneOwnedUniformSource::MaterialParameter { authored_name });
    }
    if let Some((channel, resolution)) = scene_audio_spectrum_source(name) {
        require_uniform_array_shape(
            key,
            name,
            member,
            SceneShaderScalarType::F32,
            resolution.band_count(),
            16,
        )?;
        return Ok(SceneOwnedUniformSource::AudioSpectrum {
            channel,
            resolution,
        });
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
        "g_ModelViewProjectionMatrix" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::ModelViewProjectionMatrix)
        }
        "g_EffectModelViewProjectionMatrix" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::EffectModelViewProjectionMatrix)
        }
        "g_EffectTextureProjectionMatrixInverse" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::EffectTextureProjectionMatrixInverse)
        }
        "g_LayerModelMatrix" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 4, 64)?;
            Ok(SceneOwnedUniformSource::LayerModelMatrix)
        }
        "g_Color4" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 1, 16)?;
            Ok(SceneOwnedUniformSource::ObjectColor4)
        }
        "g_Alpha" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 1, 1, 4)?;
            Ok(SceneOwnedUniformSource::ObjectAlpha)
        }
        "g_ParallaxPosition" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 2, 1, 8)?;
            Ok(SceneOwnedUniformSource::ParallaxPosition)
        }
        "g_TexelSize" => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 2, 1, 8)?;
            Ok(SceneOwnedUniformSource::CurrentRenderTargetTexelSize)
        }
        _ if name.starts_with("g_Texture") && name.ends_with("Resolution") => {
            require_uniform_shape(key, name, member, SceneShaderScalarType::F32, 4, 1, 16)?;
            let slot = name["g_Texture".len()..name.len() - "Resolution".len()]
                .parse::<u32>()
                .map_err(|_| {
                    format!("scene-owned uniform {name:?} in {key:?} has an invalid texture slot")
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
) -> Result<(), String> {
    let byte_size = array_count
        .checked_mul(array_stride)
        .ok_or_else(|| format!("scene-owned uniform {name:?} in {key:?} size overflows"))?;
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

fn scene_audio_spectrum_source(
    name: &str,
) -> Option<(SceneAudioSpectrumChannel, SceneAudioSpectrumResolution)> {
    use SceneAudioSpectrumChannel::{Left, Right};
    use SceneAudioSpectrumResolution::{Bands16, Bands32, Bands64};

    match name {
        "g_AudioSpectrum16Left" => Some((Left, Bands16)),
        "g_AudioSpectrum16Right" => Some((Right, Bands16)),
        "g_AudioSpectrum32Left" => Some((Left, Bands32)),
        "g_AudioSpectrum32Right" => Some((Right, Bands32)),
        "g_AudioSpectrum64Left" => Some((Left, Bands64)),
        "g_AudioSpectrum64Right" => Some((Right, Bands64)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneStringId;

    #[test]
    fn fixed_audio_spectrum_arrays_have_typed_sources_at_every_we_resolution() {
        for (name, array_count) in [
            ("g_AudioSpectrum16Left", 16),
            ("g_AudioSpectrum16Right", 16),
            ("g_AudioSpectrum32Left", 32),
            ("g_AudioSpectrum32Right", 32),
            ("g_AudioSpectrum64Left", 64),
            ("g_AudioSpectrum64Right", 64),
        ] {
            let member = SceneShaderUniformMemberRecord {
                name: SceneStringId::NONE,
                material_parameter: SceneStringId::NONE,
                byte_offset: 0,
                byte_size: array_count * 16,
                scalar_type: SceneShaderScalarType::F32,
                rows: 1,
                columns: 1,
                array_count,
                array_stride: 16,
                matrix_stride: 0,
            };

            scene_owned_uniform_source("workshop/example/audio", name, None, &member)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }

    #[test]
    fn effect_mvp_resolves_to_an_independent_typed_source() {
        let member = SceneShaderUniformMemberRecord {
            name: SceneStringId::NONE,
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size: 64,
            scalar_type: SceneShaderScalarType::F32,
            rows: 4,
            columns: 4,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 16,
        };

        assert_eq!(
            scene_owned_uniform_source(
                "workshop/example/effect",
                "g_EffectModelViewProjectionMatrix",
                None,
                &member,
            )
            .expect("effect MVP source"),
            SceneOwnedUniformSource::EffectModelViewProjectionMatrix,
        );
    }

    #[test]
    fn inverse_effect_texture_projection_resolves_to_an_independent_typed_source() {
        let member = SceneShaderUniformMemberRecord {
            name: SceneStringId::NONE,
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size: 64,
            scalar_type: SceneShaderScalarType::F32,
            rows: 4,
            columns: 4,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 16,
        };

        assert_eq!(
            scene_owned_uniform_source(
                "workshop/example/effect",
                "g_EffectTextureProjectionMatrixInverse",
                None,
                &member,
            )
            .expect("inverse effect texture projection source"),
            SceneOwnedUniformSource::EffectTextureProjectionMatrixInverse,
        );
    }

    #[test]
    fn object_color_and_alpha_globals_have_strict_typed_sources() {
        let member = |rows, byte_size| SceneShaderUniformMemberRecord {
            name: SceneStringId::NONE,
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size,
            scalar_type: SceneShaderScalarType::F32,
            rows,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        };

        assert_eq!(
            scene_owned_uniform_source(
                "workshop/example/effect",
                "g_Color4",
                None,
                &member(4, 16),
            )
            .expect("object color source"),
            SceneOwnedUniformSource::ObjectColor4,
        );
        assert_eq!(
            scene_owned_uniform_source("workshop/example/effect", "g_Alpha", None, &member(1, 4),)
                .expect("object alpha source"),
            SceneOwnedUniformSource::ObjectAlpha,
        );
    }

    #[test]
    fn parallax_position_global_has_a_strict_typed_source() {
        let member = SceneShaderUniformMemberRecord {
            name: SceneStringId::NONE,
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size: 8,
            scalar_type: SceneShaderScalarType::F32,
            rows: 2,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        };

        assert_eq!(
            scene_owned_uniform_source(
                "workshop/example/effect",
                "g_ParallaxPosition",
                None,
                &member,
            )
            .expect("parallax position source"),
            SceneOwnedUniformSource::ParallaxPosition,
        );
    }

    #[test]
    fn current_render_target_texel_size_has_a_strict_typed_source() {
        let member = SceneShaderUniformMemberRecord {
            name: SceneStringId::NONE,
            material_parameter: SceneStringId::NONE,
            byte_offset: 0,
            byte_size: 8,
            scalar_type: SceneShaderScalarType::F32,
            rows: 2,
            columns: 1,
            array_count: 1,
            array_stride: 0,
            matrix_stride: 0,
        };

        assert_eq!(
            scene_owned_uniform_source("workshop/example/effect", "g_TexelSize", None, &member)
                .expect("current render-target texel-size source"),
            SceneOwnedUniformSource::CurrentRenderTargetTexelSize,
        );

        let mut incompatible = member;
        incompatible.rows = 4;
        incompatible.byte_size = 16;
        assert!(
            scene_owned_uniform_source(
                "workshop/example/effect",
                "g_TexelSize",
                None,
                &incompatible,
            )
            .is_err()
        );
    }
}
