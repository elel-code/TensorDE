//! Wallpaper Engine particle definition ingest and particle render-graph lowering.
//!
//! References:
//! - `reverse-engineered/tensor-wallpaper/docs/exe/particle-system.md`
//! - `reverse-engineered/tensor-wallpaper/docs/particle-format.md`

use super::*;
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    ColorWriteMask, CullMode, DepthTestMode, PassState, PipelineBlendMode, RenderGraph,
    RenderPassDrawPrimitive, RenderPassEffectVisibility, RenderPassNode, RenderPassRole,
    RenderTargetRole, TextureBindingRole,
};
use crate::engine::scene::ScenePipelineBlend;

impl WeIrBuilder {
    pub(super) fn add_particle_system(
        &mut self,
        object: u32,
        path: &str,
        instance: &Value,
    ) -> Result<(u32, u32), WeIngestError> {
        let mut ancestry = Vec::new();
        let (_, resource, material) = self.add_particle_definition(
            object,
            path,
            instance,
            crate::engine::scene::INVALID_PARTICLE_INDEX,
            WeIrParticleChildType::BuiltinDefault,
            1.0,
            10,
            &mut ancestry,
        )?;
        Ok((resource, material))
    }

    #[allow(clippy::too_many_arguments)]
    fn add_particle_definition(
        &mut self,
        object: u32,
        path: &str,
        instance: &Value,
        parent_particle_index: u32,
        child_type: WeIrParticleChildType,
        child_probability: f32,
        child_max_count: u32,
        ancestry: &mut Vec<String>,
    ) -> Result<(u32, u32, u32), WeIngestError> {
        let path = normalize_we_path(path);
        if ancestry.iter().any(|ancestor| ancestor == &path) {
            let mut cycle = ancestry.clone();
            cycle.push(path.clone());
            return Err(WeIngestError::InvalidProject(format!(
                "particle child cycle: {}",
                cycle.join(" -> ")
            )));
        }
        ancestry.push(path.clone());
        let resource = self.add_required_resource(&path, SceneResourceKind::ParticleDefinition)?;
        let payload = self.resources[resource as usize].payload.clone();
        let definition = parse_json_bytes(&path, &payload)?;
        let material_path = bound_string(definition.get("material")).ok_or_else(|| {
            WeIngestError::InvalidProject(format!("particle definition {path} has no material"))
        })?;
        let material = self.add_material(&material_path)?;
        self.preserve_particle_material_rgba(material)?;
        let children = parse_children(&definition, &path)?;
        let projection_mode = self.scene.orthogonal_projection_auto
            || self.scene.logical_width != 0
            || self.scene.logical_height != 0;
        let initializers = parse_initializers(&definition, projection_mode);
        let instance_color = particle_instance_color(instance);
        let system = WeIrParticleSystem {
            object,
            resource,
            material,
            parent_particle_index,
            child_type,
            child_probability,
            child_max_count,
            flags: value_u32(definition.get("flags")).unwrap_or(0),
            max_count: value_u32(definition.get("maxcount")).unwrap_or(0),
            animation_mode: particle_animation_mode(&definition, &path)?,
            sequence_multiplier: value_f32(definition.get("sequencemultiplier")).unwrap_or(1.0),
            start_time: value_f32(definition.get("starttime")).unwrap_or(0.0),
            instance_time_scale: particle_instance_time_scale(instance_color),
            instance_color,
            color_reference: particle_color_reference(&initializers),
            instance_count_scale: particle_instance_count_scale(instance),
            control_points: parse_control_points(&definition, instance),
            emitters: parse_emitters(&definition),
            initializers,
            operators: parse_operators(&definition, projection_mode),
            renderers: parse_renderers(&definition)?,
            children: children.clone(),
        };
        if system.falling_leaves_profile().is_none()
            && system.ambient_sparkles_profile().is_none()
            && system.floral_oscillation_profile().is_none()
            && system.module_sprite_profile().is_none()
        {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("particle-profile-not-yet-gpu-specialized:{path}"),
                expected_subsystem: "scene RenderingDevice particle specialization".to_owned(),
                containment: "typed-particle-ir-retained-without-runtime-draw".to_owned(),
            });
        }
        let particle_index = self.particles.len() as u32;
        self.particles.push(system);
        for child in children {
            self.add_particle_definition(
                object,
                &child.particle,
                instance,
                particle_index,
                child.child_type,
                child.probability,
                child.max_count,
                ancestry,
            )?;
        }
        ancestry.pop();
        Ok((particle_index, resource, material))
    }

    pub(super) fn add_particle_render_graph(&mut self, object: u32, color_blend_mode: i32) -> u32 {
        let graph_index = self.render_graphs.len() as u32;
        let passes = self
            .particles
            .iter()
            .enumerate()
            .filter(|(_, particle)| {
                particle.object == object
                    && (particle.parent_particle_index
                        == crate::engine::scene::INVALID_PARTICLE_INDEX
                        || particle.child_type == WeIrParticleChildType::BuiltinDefault)
            })
            .map(|(particle_index, particle)| {
                let material = particle.material;
                let material_pass = self
                    .materials
                    .get(material as usize)
                    .and_then(|material| self.material_passes.get(material.pass_start as usize));
                RenderPassNode {
                    id: particle_index as u32,
                    role: RenderPassRole::Particle,
                    draw_primitive: RenderPassDrawPrimitive::ParticleBillboard,
                    object_index: Some(object as usize),
                    material_index: Some(material as usize),
                    pass_index: 0,
                    shader: material_pass.map(|pass| pass.shader_key.clone()),
                    target: RenderTargetRole::SceneColor,
                    target_name: None,
                    target_extent: None,
                    target_format: None,
                    bindings: vec![TextureBindingRole::SourceTexture],
                    effect_visibility: RenderPassEffectVisibility::NONE,
                    state: particle_pass_state(
                        particle.renderers.first().and_then(particle_renderer_blend),
                        material_pass,
                        color_blend_mode,
                    ),
                }
            })
            .collect();
        self.render_graphs.push(RenderGraph {
            activation_policy: Default::default(),
            passes,
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        });
        graph_index
    }
}

pub(super) fn particle_animation_mode(
    definition: &Value,
    definition_path: &str,
) -> Result<crate::engine::scene::SceneParticleAnimationMode, WeIngestError> {
    let Some(value) = definition.get("animationmode") else {
        return Ok(crate::engine::scene::SceneParticleAnimationMode::InterpolatedSequence);
    };
    let mode = bound_string(Some(value)).ok_or_else(|| {
        WeIngestError::InvalidProject(format!(
            "particle definition {definition_path} has a non-string animationmode"
        ))
    })?;
    match mode.as_str() {
        "sequence" => Ok(crate::engine::scene::SceneParticleAnimationMode::InterpolatedSequence),
        "randomframe" => Ok(crate::engine::scene::SceneParticleAnimationMode::RandomFrame),
        _ => Err(WeIngestError::InvalidProject(format!(
            "particle definition {definition_path} has unsupported animationmode {mode:?}"
        ))),
    }
}

fn particle_pass_state(
    renderer_blend: Option<ScenePipelineBlend>,
    pass: Option<&WeIrMaterialPass>,
    color_blend_mode: i32,
) -> PassState {
    PassState {
        pipeline_blend: renderer_blend.map_or_else(
            || {
                pass.map_or(PipelineBlendMode::Translucent, |pass| {
                    pipeline_blend_mode(pass.pipeline_blend)
                })
            },
            pipeline_blend_mode,
        ),
        scene_blend: match scene_blend_from_color_blend_mode(color_blend_mode) {
            SceneBlendMode::Alpha => SceneBlendMode::Alpha,
            blend => blend,
        },
        shader_blend: None,
        depth_test: if pass.is_some_and(|pass| pass.depth_test == SceneDepthTest::Enabled) {
            DepthTestMode::LessEqual
        } else {
            DepthTestMode::Disabled
        },
        depth_write: pass.is_some_and(|pass| pass.depth_write),
        cull_mode: if pass.is_some_and(|pass| pass.cull_mode == SceneCullMode::Normal) {
            CullMode::Back
        } else {
            CullMode::None
        },
        // WE's generic particle pipelines preserve the SceneColor alpha channel;
        // every verified sprite and SpriteTrail draw uses D3D11 write mask 0x7.
        color_write_mask: ColorWriteMask::Rgb,
        ..PassState::default()
    }
}

fn pipeline_blend_mode(blend: ScenePipelineBlend) -> PipelineBlendMode {
    match blend {
        ScenePipelineBlend::Normal => PipelineBlendMode::Normal,
        ScenePipelineBlend::Translucent => PipelineBlendMode::Translucent,
        ScenePipelineBlend::Additive => PipelineBlendMode::Additive,
        ScenePipelineBlend::Disabled => PipelineBlendMode::Disabled,
        ScenePipelineBlend::AlphaToCoverage => PipelineBlendMode::AlphaToCoverage,
    }
}

fn particle_renderer_blend(renderer: &WeIrParticleRenderer) -> Option<ScenePipelineBlend> {
    match renderer {
        WeIrParticleRenderer::Sprite { blending, .. }
        | WeIrParticleRenderer::SpriteTrail { blending, .. } => *blending,
        WeIrParticleRenderer::Unsupported { .. } => None,
    }
}

fn parse_control_points(definition: &Value, instance: &Value) -> Vec<WeIrParticleControlPoint> {
    definition
        .get("controlpoint")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let origin_override = format!("controlpoint{id}");
            let angles_override = format!("controlpointangle{id}");
            WeIrParticleControlPoint {
                id,
                origin: instance
                    .pointer(&format!("/instanceoverride/{origin_override}"))
                    .and_then(|value| particle_vec3(Some(value)))
                    .or_else(|| particle_vec3(value.get("origin")))
                    .unwrap_or_default(),
                angles: instance
                    .pointer(&format!("/instanceoverride/{angles_override}"))
                    .and_then(|value| particle_vec3(Some(value)))
                    .or_else(|| particle_vec3(value.get("angles")))
                    .unwrap_or_default(),
            }
        })
        .collect()
}

fn parse_emitters(definition: &Value) -> Vec<WeIrParticleEmitter> {
    particle_modules(definition, "emitter")
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let name = module_name(value);
            let origin = particle_vec3(value.get("origin")).unwrap_or_default();
            let directions = particle_vec3(value.get("directions")).unwrap_or(SceneVec3::ONE);
            let distance_min = particle_vec3(value.get("distancemin")).unwrap_or_default();
            let distance_max = particle_vec3(value.get("distancemax")).unwrap_or_default();
            let rate = value_f32(value.get("rate")).unwrap_or(0.0);
            match name.as_str() {
                "sphererandom" => WeIrParticleEmitter::SphereRandom {
                    id,
                    rate,
                    instantaneous: value_u32(value.get("instantaneous")).unwrap_or(0),
                    origin,
                    directions,
                    distance_min,
                    distance_max,
                    speed_min: value_f32(value.get("speedmin")).unwrap_or(0.0),
                    speed_max: value_f32(value.get("speedmax")).unwrap_or(0.0),
                },
                "boxrandom" => WeIrParticleEmitter::BoxRandom {
                    id,
                    rate,
                    origin,
                    directions,
                    distance_min,
                    distance_max,
                },
                _ => WeIrParticleEmitter::Unsupported { id, name },
            }
        })
        .collect()
}

pub(super) fn parse_initializers(
    definition: &Value,
    orthogonal_projection_has_extent: bool,
) -> Vec<WeIrParticleInitializer> {
    let (default_turbulent_speed_min, default_turbulent_speed_max) =
        if orthogonal_projection_has_extent {
            (100.0, 250.0)
        } else {
            (0.5, 1.0)
        };
    particle_modules(definition, "initializer")
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let name = module_name(value);
            let scalar_min = value_f32(value.get("min")).unwrap_or(0.0);
            let scalar_max = value_f32(value.get("max")).unwrap_or(scalar_min);
            let vector_min = particle_vec3(value.get("min")).unwrap_or_default();
            let vector_max = particle_vec3(value.get("max")).unwrap_or(vector_min);
            match name.as_str() {
                "lifetimerandom" => WeIrParticleInitializer::LifetimeRandom {
                    id,
                    min: scalar_min,
                    max: scalar_max,
                },
                "sizerandom" => WeIrParticleInitializer::SizeRandom {
                    id,
                    min: scalar_min,
                    max: scalar_max,
                },
                "velocityrandom" => WeIrParticleInitializer::VelocityRandom {
                    id,
                    min: vector_min,
                    max: vector_max,
                },
                "colorrandom" => WeIrParticleInitializer::ColorRandom {
                    id,
                    min: vector_min,
                    max: vector_max,
                },
                "alpharandom" => WeIrParticleInitializer::AlphaRandom {
                    id,
                    min: scalar_min,
                    max: scalar_max,
                },
                "rotationrandom" => WeIrParticleInitializer::RotationRandom {
                    id,
                    min: value_f32(value.get("min")).unwrap_or(0.0),
                    max: value_f32(value.get("max")).unwrap_or(std::f32::consts::TAU),
                },
                "turbulentvelocityrandom" => WeIrParticleInitializer::TurbulentVelocityRandom {
                    id,
                    offset: value_f32(value.get("offset")).unwrap_or(0.0),
                    scale: value_f32(value.get("scale")).unwrap_or(1.0),
                    speed_min: value_f32(value.get("speedmin"))
                        .unwrap_or(default_turbulent_speed_min),
                    speed_max: value_f32(value.get("speedmax"))
                        .unwrap_or(default_turbulent_speed_max),
                    phase_min: value_f32(value.get("phasemin")).unwrap_or(0.0),
                    phase_max: value_f32(value.get("phasemax")).unwrap_or(0.0),
                    time_scale: value_f32(value.get("timescale")).unwrap_or(1.0),
                    right: particle_vec3(value.get("right")).unwrap_or(SceneVec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    }),
                    forward: particle_vec3(value.get("forward")).unwrap_or(SceneVec3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                },
                "angularvelocityrandom" => WeIrParticleInitializer::AngularVelocityRandom {
                    id,
                    min: vector_min,
                    max: vector_max,
                },
                "inheritinitialvaluefromevent" => {
                    WeIrParticleInitializer::InheritInitialValueFromEvent {
                        id,
                        input: bound_string(value.get("input")).unwrap_or_default(),
                    }
                }
                _ => WeIrParticleInitializer::Unsupported { id, name },
            }
        })
        .collect()
}

pub(super) fn parse_operators(
    definition: &Value,
    orthogonal_projection_has_extent: bool,
) -> Vec<WeIrParticleOperator> {
    let (default_time_scale, default_scale, default_speed_min, default_speed_max) =
        if orthogonal_projection_has_extent {
            (20.0, 0.01, 500.0, 1000.0)
        } else {
            (1.0, 1.0, 1.0, 2.0)
        };
    particle_modules(definition, "operator")
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let name = module_name(value);
            match name.as_str() {
                "movement" => WeIrParticleOperator::Movement {
                    id,
                    gravity: particle_vec3(value.get("gravity")).unwrap_or_default(),
                },
                "alphafade" => WeIrParticleOperator::AlphaFade {
                    id,
                    fade_in_time: value_f32(value.get("fadeintime")).unwrap_or(0.0),
                    fade_out_time: value_f32(value.get("fadeouttime")).unwrap_or(1.0),
                },
                "angularmovement" => WeIrParticleOperator::AngularMovement { id },
                "oscillatealpha" => WeIrParticleOperator::OscillateAlpha {
                    id,
                    frequency_min: value_f32(value.get("frequencymin")).unwrap_or(1.0),
                    frequency_max: value_f32(value.get("frequencymax")).unwrap_or(10.0),
                    phase_min: value_f32(value.get("phasemin")).unwrap_or(0.0),
                    phase_max: value_f32(value.get("phasemax")).unwrap_or(std::f32::consts::TAU),
                    scale_min: value_f32(value.get("scalemin")).unwrap_or(0.0),
                    scale_max: value_f32(value.get("scalemax")).unwrap_or(1.0),
                },
                "oscillateposition" => WeIrParticleOperator::OscillatePosition {
                    id,
                    frequency_min: value_f32(value.get("frequencymin")).unwrap_or(0.0),
                    frequency_max: value_f32(value.get("frequencymax")).unwrap_or(5.0),
                    phase_min: value_f32(value.get("phasemin")).unwrap_or(0.0),
                    phase_max: value_f32(value.get("phasemax")).unwrap_or(std::f32::consts::TAU),
                    scale_min: value_f32(value.get("scalemin")).unwrap_or(0.0),
                    scale_max: value_f32(value.get("scalemax")).unwrap_or(10.0),
                    mask: particle_vec3(value.get("mask")).unwrap_or(SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                },
                "oscillatesize" => WeIrParticleOperator::OscillateSize {
                    id,
                    frequency_min: value_f32(value.get("frequencymin")).unwrap_or(0.0),
                    frequency_max: value_f32(value.get("frequencymax")).unwrap_or(10.0),
                    phase_min: value_f32(value.get("phasemin")).unwrap_or(0.0),
                    phase_max: value_f32(value.get("phasemax")).unwrap_or(std::f32::consts::TAU),
                    scale_min: value_f32(value.get("scalemin")).unwrap_or(0.8),
                    scale_max: value_f32(value.get("scalemax")).unwrap_or(1.2),
                },
                "maintaindistancetocontrolpoint" => {
                    WeIrParticleOperator::MaintainDistanceToControlPoint {
                        id,
                        control_point: value_u32(value.get("controlpoint")).unwrap_or(0),
                        distance: value_f32(value.get("distance")).unwrap_or(0.0),
                        variable_strength: value_f32(value.get("variablestrength")).unwrap_or(0.0),
                    }
                }
                "controlpointattract" => WeIrParticleOperator::ControlPointAttract {
                    id,
                    control_point: value_u32(value.get("controlpoint")).unwrap_or(0),
                    origin: particle_vec3(value.get("origin")).unwrap_or_default(),
                    scale: value_f32(value.get("scale")).unwrap_or(0.0),
                    threshold: value_f32(value.get("threshold")).unwrap_or(0.0),
                },
                "turbulence" => WeIrParticleOperator::Turbulence {
                    id,
                    mask: particle_vec3(value.get("mask")).unwrap_or(SceneVec3::ONE),
                    phase_min: value_f32(value.get("phasemin")).unwrap_or(0.0),
                    phase_max: value_f32(value.get("phasemax")).unwrap_or(0.0),
                    scale: value_f32(value.get("scale")).unwrap_or(default_scale),
                    speed_min: value_f32(value.get("speedmin")).unwrap_or(default_speed_min),
                    speed_max: value_f32(value.get("speedmax")).unwrap_or(default_speed_max),
                    time_scale: value_f32(value.get("timescale")).unwrap_or(default_time_scale),
                },
                "sizechange" => WeIrParticleOperator::SizeChange {
                    id,
                    start_time: value_f32(value.get("starttime")).unwrap_or(0.0),
                    start_value: value_f32(value.get("startvalue")).unwrap_or(1.0),
                    end_value: value_f32(value.get("endvalue")).unwrap_or(0.0),
                },
                "vortex_v2" => WeIrParticleOperator::Vortex {
                    id,
                    axis: particle_vec3(value.get("axis")).unwrap_or(SceneVec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    }),
                    distance_inner: value_f32(value.get("distanceinner")).unwrap_or(0.0),
                    distance_outer: value_f32(value.get("distanceouter")).unwrap_or(0.0),
                    speed_inner: value_f32(value.get("speedinner")).unwrap_or(0.0),
                    speed_outer: value_f32(value.get("speedouter")).unwrap_or(0.0),
                },
                _ => WeIrParticleOperator::Unsupported { id, name },
            }
        })
        .collect()
}

pub(super) fn parse_renderers(
    definition: &Value,
) -> Result<Vec<WeIrParticleRenderer>, WeIngestError> {
    particle_modules(definition, "renderer")
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let name = module_name(value);
            let blending = parse_particle_renderer_blend(value.get("blending"))?;
            Ok(match name.as_str() {
                "sprite" => WeIrParticleRenderer::Sprite {
                    id,
                    flags: value_u32(value.get("flags")).unwrap_or(0),
                    blending,
                },
                "spritetrail" => WeIrParticleRenderer::SpriteTrail {
                    id,
                    flags: value_u32(value.get("flags")).unwrap_or(0),
                    blending,
                    length: value_f32(value.get("length")).unwrap_or(0.0),
                    min_length: value_f32(value.get("minlength")).unwrap_or(0.0),
                    // The WE SpriteTrail renderer registers ten as its
                    // longitudinal clamp even when the authored JSON omits it.
                    max_length: 10.0,
                },
                _ => WeIrParticleRenderer::Unsupported { id, name },
            })
        })
        .collect()
}

fn parse_particle_renderer_blend(
    value: Option<&Value>,
) -> Result<Option<ScenePipelineBlend>, WeIngestError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let blend = bound_string(Some(value)).ok_or_else(|| {
        WeIngestError::InvalidProject("particle renderer blending must be a string".to_owned())
    })?;
    match blend.to_ascii_lowercase().as_str() {
        "normal" => Ok(Some(ScenePipelineBlend::Normal)),
        "translucent" => Ok(Some(ScenePipelineBlend::Translucent)),
        "additive" => Ok(Some(ScenePipelineBlend::Additive)),
        _ => Err(WeIngestError::InvalidProject(format!(
            "particle renderer has unsupported blending {blend:?}"
        ))),
    }
}

fn parse_children(
    definition: &Value,
    definition_path: &str,
) -> Result<Vec<WeIrParticleChild>, WeIngestError> {
    particle_modules(definition, "children")
        .map(|value| {
            let raw_type = module_name_from(value.get("type"));
            let child_type = match raw_type.as_str() {
                "" if value.get("type").is_none() => WeIrParticleChildType::BuiltinDefault,
                "static" => WeIrParticleChildType::Static,
                "eventfollow" => WeIrParticleChildType::EventFollow,
                "eventspawn" => WeIrParticleChildType::EventSpawn,
                "eventdeath" => WeIrParticleChildType::EventDeath,
                _ => {
                    return Err(WeIngestError::InvalidProject(format!(
                        "particle definition {definition_path} has invalid child type {raw_type:?}"
                    )));
                }
            };
            let particle = bound_string(value.get("name")).ok_or_else(|| {
                WeIngestError::InvalidProject(format!(
                    "particle definition {definition_path} has child without a particle name"
                ))
            })?;
            Ok(WeIrParticleChild {
                id: value_u32(value.get("id")).unwrap_or(0),
                particle,
                child_type,
                max_count: value_u32(value.get("maxcount")).unwrap_or(10),
                probability: value_f32(value.get("probability")).unwrap_or(1.0),
            })
        })
        .collect()
}

fn particle_modules<'a>(definition: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    definition
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn module_name(value: &Value) -> String {
    module_name_from(value.get("name"))
}

fn module_name_from(value: Option<&Value>) -> String {
    bound_string(value).unwrap_or_default().to_ascii_lowercase()
}

fn particle_vec3(value: Option<&Value>) -> Option<SceneVec3> {
    parse_vec3(value).or_else(|| {
        value_f32(value).map(|scalar| SceneVec3 {
            x: scalar,
            y: scalar,
            z: scalar,
        })
    })
}

pub(super) fn particle_instance_color(instance: &Value) -> Option<SceneVec3> {
    let overrides = instance.get("instanceoverride")?;
    if let Some(color) = particle_vec3(overrides.get("colorn")) {
        return valid_instance_color(color);
    }
    particle_vec3(overrides.get("color")).and_then(|color| {
        valid_instance_color(SceneVec3 {
            x: legacy_color_component(color.x),
            y: legacy_color_component(color.y),
            z: legacy_color_component(color.z),
        })
    })
}

fn valid_instance_color(color: SceneVec3) -> Option<SceneVec3> {
    (color.x.is_finite() && color.y.is_finite() && color.z.is_finite()).then_some(color)
}

fn legacy_color_component(component: f32) -> f32 {
    (component / 255.0 * 100_000.0).round() / 100_000.0
}

pub(super) fn particle_instance_time_scale(instance_color: Option<SceneVec3>) -> f32 {
    instance_color
        .map(|color| color.x.max(0.01))
        .filter(|scale| scale.is_finite())
        .unwrap_or(1.0)
}

fn particle_color_reference(initializers: &[WeIrParticleInitializer]) -> SceneVec3 {
    initializers
        .iter()
        .find_map(|initializer| match initializer {
            WeIrParticleInitializer::ColorRandom { min, max, .. } => {
                let min = normalized_particle_color(*min);
                let max = normalized_particle_color(*max);
                Some(SceneVec3 {
                    x: (min.x + max.x) * 0.5,
                    y: (min.y + max.y) * 0.5,
                    z: (min.z + max.z) * 0.5,
                })
            }
            _ => None,
        })
        .unwrap_or(SceneVec3::ONE)
}

fn normalized_particle_color(color: SceneVec3) -> SceneVec3 {
    let divisor = if color.x > 1.0 || color.y > 1.0 || color.z > 1.0 {
        255.0
    } else {
        1.0
    };
    SceneVec3 {
        x: color.x / divisor,
        y: color.y / divisor,
        z: color.z / divisor,
    }
}

fn particle_instance_count_scale(instance: &Value) -> f32 {
    instance
        .pointer("/instanceoverride/count")
        .and_then(|value| value_f32(Some(value)))
        .filter(|scale| scale.is_finite() && *scale >= 0.0)
        .unwrap_or(1.0)
}
