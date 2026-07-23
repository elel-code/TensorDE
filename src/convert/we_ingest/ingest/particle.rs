//! Wallpaper Engine particle definition ingest and particle render-graph lowering.
//!
//! References:
//! - `reverse-engineered/docs/exe/particle-system.md`
//! - `reverse-engineered/docs/particle-format.md`

use super::*;
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    CullMode, DepthTestMode, PassState, PipelineBlendMode, RenderGraph, RenderPassDrawPrimitive,
    RenderPassEffectVisibility, RenderPassNode, RenderPassRole, RenderTargetRole,
    TextureBindingRole,
};
use crate::engine::scene::ScenePipelineBlend;

impl WeIrBuilder {
    pub(super) fn add_particle_system(
        &mut self,
        object: u32,
        path: &str,
        instance: &Value,
    ) -> Result<(u32, u32), WeIngestError> {
        let path = normalize_we_path(path);
        let resource = self.add_required_resource(&path, SceneResourceKind::ParticleDefinition)?;
        let payload = self.resources[resource as usize].payload.clone();
        let definition = parse_json_bytes(&path, &payload)?;
        let material_path = bound_string(definition.get("material")).ok_or_else(|| {
            WeIngestError::InvalidProject(format!("particle definition {path} has no material"))
        })?;
        let material = self.add_material(&material_path)?;
        let system = WeIrParticleSystem {
            object,
            resource,
            material,
            flags: value_u32(definition.get("flags")).unwrap_or(0),
            max_count: value_u32(definition.get("maxcount")).unwrap_or(0),
            sequence_multiplier: value_f32(definition.get("sequencemultiplier")).unwrap_or(1.0),
            start_time: value_f32(definition.get("starttime")).unwrap_or(0.0),
            instance_time_scale: particle_instance_time_scale(instance),
            control_points: parse_control_points(&definition, instance),
            emitters: parse_emitters(&definition),
            initializers: parse_initializers(&definition),
            operators: parse_operators(&definition),
            renderers: parse_renderers(&definition),
            children: parse_children(&definition),
        };
        if system.falling_leaves_profile().is_none()
            && system.ambient_sparkles_profile().is_none()
            && system.floral_oscillation_profile().is_none()
        {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("particle-profile-not-yet-gpu-specialized:{path}"),
                expected_subsystem: "scene RenderingDevice particle specialization".to_owned(),
                containment: "typed-particle-ir-retained-without-runtime-draw".to_owned(),
            });
        }
        self.particles.push(system);
        Ok((resource, material))
    }

    pub(super) fn add_particle_render_graph(
        &mut self,
        object: u32,
        material: u32,
        color_blend_mode: i32,
    ) -> u32 {
        let graph_index = self.render_graphs.len() as u32;
        let pass = self
            .materials
            .get(material as usize)
            .and_then(|material| self.material_passes.get(material.pass_start as usize));
        let state = PassState {
            pipeline_blend: pass.map_or(PipelineBlendMode::Translucent, |pass| {
                match pass.pipeline_blend {
                    ScenePipelineBlend::Normal => PipelineBlendMode::Normal,
                    ScenePipelineBlend::Translucent => PipelineBlendMode::Translucent,
                    ScenePipelineBlend::Additive => PipelineBlendMode::Additive,
                    ScenePipelineBlend::Disabled => PipelineBlendMode::Disabled,
                    ScenePipelineBlend::AlphaToCoverage => PipelineBlendMode::AlphaToCoverage,
                }
            }),
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
            ..PassState::default()
        };
        self.render_graphs.push(RenderGraph {
            activation_policy: Default::default(),
            passes: vec![RenderPassNode {
                id: 0,
                role: RenderPassRole::Particle,
                draw_primitive: RenderPassDrawPrimitive::ParticleBillboard,
                object_index: Some(object as usize),
                material_index: Some(material as usize),
                pass_index: 0,
                shader: pass.map(|pass| pass.shader_key.clone()),
                target: RenderTargetRole::SceneColor,
                target_name: None,
                target_extent: None,
                target_format: None,
                bindings: vec![TextureBindingRole::SourceTexture],
                effect_visibility: RenderPassEffectVisibility::NONE,
                state,
            }],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        });
        graph_index
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

fn parse_initializers(definition: &Value) -> Vec<WeIrParticleInitializer> {
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
                    scale: value_f32(value.get("scale")).unwrap_or(0.0),
                    speed_min: value_f32(value.get("speedmin")).unwrap_or(0.0),
                    speed_max: value_f32(value.get("speedmax")).unwrap_or(0.0),
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

fn parse_operators(definition: &Value) -> Vec<WeIrParticleOperator> {
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
                _ => WeIrParticleOperator::Unsupported { id, name },
            }
        })
        .collect()
}

fn parse_renderers(definition: &Value) -> Vec<WeIrParticleRenderer> {
    particle_modules(definition, "renderer")
        .map(|value| {
            let id = value_u32(value.get("id")).unwrap_or(0);
            let name = module_name(value);
            if name == "sprite" {
                WeIrParticleRenderer::Sprite {
                    id,
                    flags: value_u32(value.get("flags")).unwrap_or(0),
                }
            } else {
                WeIrParticleRenderer::Unsupported { id, name }
            }
        })
        .collect()
}

fn parse_children(definition: &Value) -> Vec<WeIrParticleChild> {
    particle_modules(definition, "children")
        .map(|value| WeIrParticleChild {
            id: value_u32(value.get("id")).unwrap_or(0),
            particle: bound_string(value.get("name")).unwrap_or_default(),
            event: match module_name_from(value.get("type")).as_str() {
                "eventbirth" => WeIrParticleChildEvent::Birth,
                "eventdeath" => WeIrParticleChildEvent::Death,
                "eventcollision" => WeIrParticleChildEvent::Collision,
                _ => WeIrParticleChildEvent::Unknown,
            },
            max_count: value_u32(value.get("maxcount")).unwrap_or(0),
            probability: value_f32(value.get("probability")).unwrap_or(1.0),
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

fn particle_instance_time_scale(instance: &Value) -> f32 {
    instance
        .pointer("/instanceoverride/colorn")
        .and_then(|value| particle_vec3(Some(value)))
        .map(|color| ((color.x / 255.0 * 100_000.0).round() / 100_000.0).max(0.01))
        .filter(|scale| scale.is_finite())
        .unwrap_or(1.0)
}
