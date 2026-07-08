//! Engine-owned WE effect pass graph.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{
    SCENE_WE_PASS_INPUT_TEXTURE_SLOT, SceneAlphaWriteMode, SceneCullMode, SceneDepthTest,
    SceneEffectCommand, SceneEffectConstantValue, SceneEffectFboBinding, SceneEffectFboFormat,
    SceneEffectImageRef, SceneEffectPassBlend, SceneEffectTextureResourceBinding, SceneGraphTarget,
    SceneImageLayerPassTarget, SceneImageLayerTargetPlan, SceneObject, SceneObjectEffectProgram,
    SceneObjectId, SceneResourceId, image_layer_pass_target, we::WeEffectKind,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectPassGraphPlan {
    pub object_program_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub target_count: usize,
    pub input_binding_count: usize,
    pub resident_texture_binding_count: usize,
    pub image_layer_target_count: usize,
    pub image_layer_scene_output_pass_count: usize,
    pub image_layer_targets: Vec<SceneImageLayerTargetPlan>,
    pub targets: Vec<SceneEffectPassGraphTarget>,
    pub passes: Vec<SceneEffectPassGraphMaterialPass>,
    pub copies: Vec<SceneEffectPassGraphCopy>,
    pub swaps: Vec<SceneEffectPassGraphSwap>,
    pub command_order: [&'static str; 10],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectPassGraphTarget {
    pub target: SceneGraphTarget,
    pub object: SceneObjectId,
    pub program_index: usize,
    pub name: String,
    pub format: Option<SceneEffectFboFormat>,
    pub scale: f32,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectPassGraphMaterialPass {
    pub graph_command_index: usize,
    pub graph_pass_index: usize,
    pub object: SceneObjectId,
    pub program_index: usize,
    pub pass_index: usize,
    pub effect_file: String,
    pub effect: WeEffectKind,
    pub shader: Option<String>,
    pub source: Option<SceneEffectPassGraphInputBinding>,
    pub input_bindings: Vec<SceneEffectPassGraphInputBinding>,
    pub output: SceneEffectPassGraphOutput,
    pub blend: SceneEffectPassBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_write: SceneAlphaWriteMode,
    pub texture_resources: Vec<SceneEffectTextureResourceBinding>,
    pub combos: BTreeMap<String, i64>,
    pub constants: BTreeMap<String, SceneEffectConstantValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectPassGraphInputBinding {
    pub slot: u32,
    pub image: SceneEffectImageRef,
    pub source: SceneEffectPassGraphInputSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneEffectPassGraphInputSource {
    ObjectSourceTexture(SceneResourceId),
    GraphTarget(SceneGraphTarget),
    PreviousFramebuffer,
    Scene,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneEffectPassGraphOutput {
    GraphTarget(SceneGraphTarget),
    ObjectFinal(SceneObjectId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectPassGraphSwap {
    pub graph_command_index: usize,
    pub object: SceneObjectId,
    pub program_index: usize,
    pub pass_index: usize,
    pub a: SceneGraphTarget,
    pub b: SceneGraphTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectPassGraphCopy {
    pub graph_command_index: usize,
    pub object: SceneObjectId,
    pub program_index: usize,
    pub pass_index: usize,
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
}

impl SceneEffectPassGraphPlan {
    pub fn empty() -> Self {
        Self {
            object_program_count: 0,
            material_pass_count: 0,
            copy_command_count: 0,
            swap_command_count: 0,
            target_count: 0,
            input_binding_count: 0,
            resident_texture_binding_count: 0,
            image_layer_target_count: 0,
            image_layer_scene_output_pass_count: 0,
            image_layer_targets: Vec::new(),
            targets: Vec::new(),
            passes: Vec::new(),
            copies: Vec::new(),
            swaps: Vec::new(),
            command_order: [
                "collect_scene_object_effect_programs",
                "declare_named_fbo_targets",
                "count_we_image_layer_scene_output_passes",
                "resolve_we_image_layer_source_target_ping_pong",
                "resolve_effect_material_pass_sources",
                "resolve_effect_material_pass_targets",
                "preserve_effect_texture_resource_bindings",
                "record_effect_copy_commands_without_draws",
                "record_effect_swap_commands_without_draws",
                "emit_scene_effect_pass_graph",
            ],
        }
    }

    pub fn from_scene(
        objects: &[SceneObject],
        effects: &[SceneObjectEffectProgram],
    ) -> Result<Self, String> {
        if effects.is_empty() {
            return Ok(Self::empty());
        }

        let objects_by_id = objects
            .iter()
            .map(|object| (object.id, object))
            .collect::<BTreeMap<_, _>>();
        let image_layer_pass_counts = image_layer_scene_output_pass_counts(effects);
        let mut image_layer_pass_indices = BTreeMap::<SceneObjectId, usize>::new();
        let mut plan = Self::empty();
        plan.object_program_count = effects.len();
        for (object_id, pass_count) in &image_layer_pass_counts {
            let object = objects_by_id.get(object_id).ok_or_else(|| {
                format!("scene effect program references missing object {object_id:?}")
            })?;
            if let Some(target_plan) =
                SceneImageLayerTargetPlan::for_object(*object_id, object.source, *pass_count)
            {
                plan.image_layer_scene_output_pass_count = plan
                    .image_layer_scene_output_pass_count
                    .saturating_add(target_plan.scene_output_pass_count);
                plan.image_layer_targets.push(target_plan);
            }
        }
        plan.image_layer_target_count = plan.image_layer_targets.len();

        for (program_index, object_program) in effects.iter().enumerate() {
            let object = objects_by_id.get(&object_program.object).ok_or_else(|| {
                format!(
                    "scene effect program references missing object {:?}",
                    object_program.object
                )
            })?;
            let mut fbo_targets =
                effect_program_fbo_targets(&mut plan, program_index, object_program);

            for command in &object_program.program.commands {
                let graph_command_index = plan
                    .material_pass_count
                    .saturating_add(plan.copy_command_count)
                    .saturating_add(plan.swap_command_count);
                match command {
                    SceneEffectCommand::MaterialPass(pass) => {
                        let image_layer_target = image_layer_pass_target_for_material_pass(
                            object.id,
                            pass,
                            &image_layer_pass_counts,
                            &mut image_layer_pass_indices,
                        );
                        let material = effect_material_pass_graph(
                            &plan,
                            graph_command_index,
                            object,
                            object_program,
                            program_index,
                            pass,
                            &fbo_targets,
                            image_layer_target,
                        )?;
                        plan.input_binding_count = plan
                            .input_binding_count
                            .saturating_add(material.source.iter().count())
                            .saturating_add(material.input_bindings.len());
                        plan.resident_texture_binding_count = plan
                            .resident_texture_binding_count
                            .saturating_add(material.texture_resources.len());
                        plan.passes.push(material);
                        plan.material_pass_count = plan.material_pass_count.saturating_add(1);
                    }
                    SceneEffectCommand::Copy(copy) => {
                        plan.copies.push(SceneEffectPassGraphCopy {
                            graph_command_index,
                            object: object.id,
                            program_index,
                            pass_index: copy.pass_index,
                            source: resolve_named_fbo_target(&fbo_targets, &copy.source)?,
                            target: resolve_named_fbo_target(&fbo_targets, &copy.target)?,
                        });
                        plan.copy_command_count = plan.copy_command_count.saturating_add(1);
                    }
                    SceneEffectCommand::Swap(swap) => {
                        let a = resolve_named_fbo_target(&fbo_targets, &swap.a)?;
                        let b = resolve_named_fbo_target(&fbo_targets, &swap.b)?;
                        plan.swaps.push(SceneEffectPassGraphSwap {
                            graph_command_index,
                            object: object.id,
                            program_index,
                            pass_index: swap.pass_index,
                            a,
                            b,
                        });
                        apply_effect_fbo_swap(&mut fbo_targets, &swap.a, &swap.b)?;
                        plan.swap_command_count = plan.swap_command_count.saturating_add(1);
                    }
                }
            }
        }

        plan.target_count = plan.targets.len();
        Ok(plan)
    }
}

fn effect_program_fbo_targets(
    plan: &mut SceneEffectPassGraphPlan,
    program_index: usize,
    object_program: &SceneObjectEffectProgram,
) -> BTreeMap<String, SceneGraphTarget> {
    let mut targets = BTreeMap::new();
    for fbo in &object_program.program.fbos {
        targets.insert(fbo.name.clone(), fbo.target);
        push_effect_target(plan, object_program.object, program_index, fbo);
    }
    targets
}

fn push_effect_target(
    plan: &mut SceneEffectPassGraphPlan,
    object: SceneObjectId,
    program_index: usize,
    fbo: &SceneEffectFboBinding,
) {
    if plan
        .targets
        .iter()
        .any(|target| target.target == fbo.target)
    {
        return;
    }
    plan.targets.push(SceneEffectPassGraphTarget {
        target: fbo.target,
        object,
        program_index,
        name: fbo.name.clone(),
        format: fbo.format.clone(),
        scale: fbo.scale,
        unique: fbo.unique,
    });
}

fn effect_material_pass_graph(
    plan: &SceneEffectPassGraphPlan,
    graph_command_index: usize,
    object: &SceneObject,
    object_program: &SceneObjectEffectProgram,
    program_index: usize,
    pass: &super::SceneEffectMaterialPass,
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    image_layer_target: Option<SceneImageLayerPassTarget>,
) -> Result<SceneEffectPassGraphMaterialPass, String> {
    let source = pass
        .source
        .as_ref()
        .map(|source| {
            resolve_effect_input_binding(
                object,
                fbo_targets,
                SCENE_WE_PASS_INPUT_TEXTURE_SLOT,
                source,
                image_layer_target,
            )
        })
        .transpose()?;
    let input_bindings = effect_pass_input_bindings(
        object,
        fbo_targets,
        pass,
        source.as_ref(),
        image_layer_target,
    )?;
    Ok(SceneEffectPassGraphMaterialPass {
        graph_command_index,
        graph_pass_index: plan.material_pass_count,
        object: object.id,
        program_index,
        pass_index: pass.pass_index,
        effect_file: object_program.program.effect_file.clone(),
        effect: object_program.program.effect,
        shader: pass.shader.clone(),
        source,
        input_bindings,
        output: effect_pass_output(
            object.id,
            fbo_targets,
            pass.target.as_ref(),
            image_layer_target,
        )?,
        blend: pass.blend,
        depth_test: pass.depth_test,
        depth_write: pass.depth_write,
        cull_mode: pass.cull_mode,
        alpha_write: pass.alpha_write,
        texture_resources: pass.texture_resources.clone(),
        combos: pass.combos.clone(),
        constants: pass.constants.clone(),
    })
}

fn apply_effect_fbo_swap(
    fbo_targets: &mut BTreeMap<String, SceneGraphTarget>,
    a: &SceneEffectImageRef,
    b: &SceneEffectImageRef,
) -> Result<(), String> {
    let SceneEffectImageRef::NamedFbo(a_name) = a else {
        return Err(format!("scene effect swap requires named FBOs, got {a:?}"));
    };
    let SceneEffectImageRef::NamedFbo(b_name) = b else {
        return Err(format!("scene effect swap requires named FBOs, got {b:?}"));
    };
    let a_target = *fbo_targets
        .get(a_name)
        .ok_or_else(|| format!("scene effect swap references undeclared FBO '{a_name}'"))?;
    let b_target = *fbo_targets
        .get(b_name)
        .ok_or_else(|| format!("scene effect swap references undeclared FBO '{b_name}'"))?;
    fbo_targets.insert(a_name.clone(), b_target);
    fbo_targets.insert(b_name.clone(), a_target);
    Ok(())
}

fn effect_pass_input_bindings(
    object: &SceneObject,
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    pass: &super::SceneEffectMaterialPass,
    source: Option<&SceneEffectPassGraphInputBinding>,
    image_layer_target: Option<SceneImageLayerPassTarget>,
) -> Result<Vec<SceneEffectPassGraphInputBinding>, String> {
    let mut used_slots = BTreeSet::new();
    if let Some(source) = source {
        used_slots.insert(source.slot);
    }
    let mut bindings = Vec::with_capacity(pass.binds.len());
    for (slot, image) in &pass.binds {
        if !used_slots.insert(*slot) {
            return Err(format!(
                "scene effect pass {} for object {:?} binds slot {} more than once",
                pass.pass_index, object.id, slot
            ));
        }
        bindings.push(resolve_effect_input_binding(
            object,
            fbo_targets,
            *slot,
            image,
            image_layer_target,
        )?);
    }
    Ok(bindings)
}

fn resolve_effect_input_binding(
    object: &SceneObject,
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    slot: u32,
    image: &SceneEffectImageRef,
    image_layer_target: Option<SceneImageLayerPassTarget>,
) -> Result<SceneEffectPassGraphInputBinding, String> {
    Ok(SceneEffectPassGraphInputBinding {
        slot,
        image: image.clone(),
        source: resolve_effect_input_source(object, fbo_targets, image, image_layer_target)?,
    })
}

fn resolve_effect_input_source(
    object: &SceneObject,
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    image: &SceneEffectImageRef,
    image_layer_target: Option<SceneImageLayerPassTarget>,
) -> Result<SceneEffectPassGraphInputSource, String> {
    match image {
        SceneEffectImageRef::SourceTexture => {
            if let Some(target) = image_layer_target {
                return Ok(SceneEffectPassGraphInputSource::GraphTarget(target.source));
            }
            object
                .source
                .map(SceneEffectPassGraphInputSource::ObjectSourceTexture)
                .ok_or_else(|| {
                    format!(
                        "scene effect for object {:?} requires a source texture but the object has none",
                        object.id
                    )
                })
        }
        SceneEffectImageRef::NamedFbo(name) => Ok(SceneEffectPassGraphInputSource::GraphTarget(
            *fbo_targets
                .get(name)
                .ok_or_else(|| format!("scene effect references undeclared FBO '{name}'"))?,
        )),
        SceneEffectImageRef::GraphTarget(target) => {
            Ok(SceneEffectPassGraphInputSource::GraphTarget(*target))
        }
        SceneEffectImageRef::PreviousFramebuffer => {
            if let Some(target) = image_layer_target {
                return Ok(SceneEffectPassGraphInputSource::GraphTarget(target.source));
            }
            Ok(SceneEffectPassGraphInputSource::PreviousFramebuffer)
        }
        SceneEffectImageRef::Scene => {
            if let Some(target) = image_layer_target {
                return Ok(SceneEffectPassGraphInputSource::GraphTarget(target.source));
            }
            Ok(SceneEffectPassGraphInputSource::Scene)
        }
    }
}

fn effect_pass_output(
    object: SceneObjectId,
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    target: Option<&SceneEffectImageRef>,
    image_layer_target: Option<SceneImageLayerPassTarget>,
) -> Result<SceneEffectPassGraphOutput, String> {
    match target {
        Some(SceneEffectImageRef::NamedFbo(name)) => Ok(SceneEffectPassGraphOutput::GraphTarget(
            *fbo_targets
                .get(name)
                .ok_or_else(|| format!("scene effect writes undeclared FBO '{name}'"))?,
        )),
        Some(SceneEffectImageRef::Scene) | None => Ok(image_layer_target
            .map(|target| SceneEffectPassGraphOutput::GraphTarget(target.output))
            .unwrap_or(SceneEffectPassGraphOutput::ObjectFinal(object))),
        Some(SceneEffectImageRef::SourceTexture) => {
            Err("scene effect source texture cannot be a render target".to_owned())
        }
        Some(SceneEffectImageRef::GraphTarget(target)) => {
            Ok(SceneEffectPassGraphOutput::GraphTarget(*target))
        }
        Some(SceneEffectImageRef::PreviousFramebuffer) => {
            Err("scene effect previous framebuffer cannot be a render target".to_owned())
        }
    }
}

fn image_layer_scene_output_pass_counts(
    effects: &[SceneObjectEffectProgram],
) -> BTreeMap<SceneObjectId, usize> {
    let mut counts = BTreeMap::<SceneObjectId, usize>::new();
    for object_program in effects {
        for command in &object_program.program.commands {
            let SceneEffectCommand::MaterialPass(pass) = command else {
                continue;
            };
            if effect_pass_writes_image_layer_scene_target(pass) {
                *counts.entry(object_program.object).or_default() += 1;
            }
        }
    }
    counts
}

fn image_layer_pass_target_for_material_pass(
    object: SceneObjectId,
    pass: &super::SceneEffectMaterialPass,
    pass_counts: &BTreeMap<SceneObjectId, usize>,
    pass_indices: &mut BTreeMap<SceneObjectId, usize>,
) -> Option<SceneImageLayerPassTarget> {
    if !effect_pass_writes_image_layer_scene_target(pass) {
        return None;
    }
    let pass_count = *pass_counts.get(&object)?;
    let pass_index = pass_indices.entry(object).or_default();
    let target = image_layer_pass_target(object, pass_count, *pass_index);
    *pass_index = pass_index.saturating_add(1);
    Some(target)
}

fn effect_pass_writes_image_layer_scene_target(pass: &super::SceneEffectMaterialPass) -> bool {
    matches!(
        pass.target.as_ref(),
        None | Some(SceneEffectImageRef::Scene)
    )
}

fn resolve_named_fbo_target(
    fbo_targets: &BTreeMap<String, SceneGraphTarget>,
    image: &SceneEffectImageRef,
) -> Result<SceneGraphTarget, String> {
    let SceneEffectImageRef::NamedFbo(name) = image else {
        return Err(format!(
            "scene effect swap requires named FBOs, got {image:?}"
        ));
    };
    fbo_targets
        .get(name)
        .copied()
        .ok_or_else(|| format!("scene effect swap references undeclared FBO '{name}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneMaterialContract, SceneMaterialRenderState, SceneObjectGeometry,
        SceneResourceId, we::WeEffectKind,
    };

    #[test]
    fn effect_pass_graph_maps_single_scene_output_to_image_layer_composite_a() {
        let object = object(SceneObjectId(4), Some(SceneResourceId(9)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/iris/effect.json".to_owned(),
                effect: WeEffectKind::Iris,
                fbos: Vec::new(),
                commands: vec![SceneEffectCommand::MaterialPass(
                    super::super::SceneEffectMaterialPass {
                        pass_index: 0,
                        shader: Some("effects/iris".to_owned()),
                        source: Some(SceneEffectImageRef::SourceTexture),
                        target: None,
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    },
                )],
            },
        }];

        let graph =
            SceneEffectPassGraphPlan::from_scene(&[object], &effects).expect("effect graph");

        assert_eq!(graph.material_pass_count, 1);
        assert_eq!(graph.passes[0].graph_command_index, 0);
        assert_eq!(graph.target_count, 0);
        assert_eq!(graph.image_layer_target_count, 1);
        assert_eq!(graph.image_layer_scene_output_pass_count, 1);
        assert_eq!(
            graph.image_layer_targets[0].prefill_target,
            SceneGraphTarget::ImageLayerSource(SceneObjectId(4))
        );
        assert_eq!(
            graph.image_layer_targets[0].final_source_target,
            SceneGraphTarget::ImageLayerCompositeA(SceneObjectId(4))
        );
        assert_eq!(
            graph.passes[0].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::ImageLayerCompositeA(
                SceneObjectId(4)
            ))
        );
        assert_eq!(
            graph.passes[0].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::ImageLayerSource(
                SceneObjectId(4)
            ))
        );
        assert_eq!(graph.passes[0].blend, SceneEffectPassBlend::NormalReplace);
    }

    #[test]
    fn effect_pass_graph_alternates_two_scene_outputs_back_to_composite_a() {
        let object = object(SceneObjectId(1336), Some(SceneResourceId(77)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/iris/effect.json".to_owned(),
                effect: WeEffectKind::Iris,
                fbos: Vec::new(),
                commands: vec![
                    SceneEffectCommand::MaterialPass(super::super::SceneEffectMaterialPass {
                        pass_index: 0,
                        shader: Some("effects/iris".to_owned()),
                        source: Some(SceneEffectImageRef::SourceTexture),
                        target: None,
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    }),
                    SceneEffectCommand::MaterialPass(super::super::SceneEffectMaterialPass {
                        pass_index: 1,
                        shader: Some("materials/util/effectpassthrough".to_owned()),
                        source: Some(SceneEffectImageRef::SourceTexture),
                        target: None,
                        blend: SceneEffectPassBlend::TranslucentAlpha,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    }),
                ],
            },
        }];

        let graph =
            SceneEffectPassGraphPlan::from_scene(&[object], &effects).expect("effect graph");

        assert_eq!(
            graph.image_layer_targets[0].prefill_target,
            SceneGraphTarget::ImageLayerCompositeA(SceneObjectId(1336))
        );
        assert_eq!(
            graph.passes[0].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::ImageLayerCompositeA(
                SceneObjectId(1336)
            ))
        );
        assert_eq!(
            graph.passes[0].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::ImageLayerSource(
                SceneObjectId(1336)
            ))
        );
        assert_eq!(
            graph.passes[1].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::ImageLayerSource(
                SceneObjectId(1336)
            ))
        );
        assert_eq!(
            graph.passes[1].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::ImageLayerCompositeA(
                SceneObjectId(1336)
            ))
        );
    }

    #[test]
    fn effect_pass_graph_resolves_image_layer_previous_and_scene_inputs_to_source_target() {
        let object = object(SceneObjectId(1530), Some(SceneResourceId(9)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/fluidsimulation/effect.json".to_owned(),
                effect: WeEffectKind::Unknown,
                fbos: Vec::new(),
                commands: vec![SceneEffectCommand::MaterialPass(
                    super::super::SceneEffectMaterialPass {
                        pass_index: 18,
                        shader: Some("effects/fluidsimulation_combine".to_owned()),
                        source: Some(SceneEffectImageRef::PreviousFramebuffer),
                        target: None,
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::from([(1, SceneEffectImageRef::Scene)]),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    },
                )],
            },
        }];

        let graph =
            SceneEffectPassGraphPlan::from_scene(&[object], &effects).expect("effect graph");

        assert_eq!(
            graph.passes[0].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::ImageLayerSource(
                SceneObjectId(1530)
            ))
        );
        assert_eq!(
            graph.passes[0].input_bindings[0].source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::ImageLayerSource(
                SceneObjectId(1530)
            ))
        );
        assert_eq!(
            graph.passes[0].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::ImageLayerCompositeA(
                SceneObjectId(1530)
            ))
        );
    }

    #[test]
    fn effect_pass_graph_preserves_named_fbos_multi_inputs_copy_and_swaps() {
        let object = object(SceneObjectId(7), Some(SceneResourceId(11)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/fluidsimulation/effect.json".to_owned(),
                effect: WeEffectKind::Unknown,
                fbos: vec![
                    fbo("_rt_SmokeVelocity1", SceneGraphTarget::NamedFbo(1)),
                    fbo("_rt_SmokeVelocity2", SceneGraphTarget::NamedFbo(2)),
                    fbo("_rt_SmokeCurl", SceneGraphTarget::NamedFbo(3)),
                ],
                commands: vec![
                    SceneEffectCommand::MaterialPass(super::super::SceneEffectMaterialPass {
                        pass_index: 1,
                        shader: Some("effects/fluidsimulation_vorticity".to_owned()),
                        source: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity1".to_owned(),
                        )),
                        target: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity2".to_owned(),
                        )),
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::from([(
                            1,
                            SceneEffectImageRef::NamedFbo("_rt_SmokeCurl".to_owned()),
                        )]),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    }),
                    SceneEffectCommand::Copy(super::super::SceneEffectCopyCommand {
                        pass_index: 18,
                        source: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity2".to_owned()),
                        target: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity1".to_owned()),
                    }),
                    SceneEffectCommand::Swap(super::super::SceneEffectSwapCommand {
                        pass_index: 19,
                        a: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity2".to_owned()),
                        b: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity1".to_owned()),
                    }),
                ],
            },
        }];

        let graph =
            SceneEffectPassGraphPlan::from_scene(&[object], &effects).expect("effect graph");

        assert_eq!(graph.target_count, 3);
        assert_eq!(graph.material_pass_count, 1);
        assert_eq!(graph.copy_command_count, 1);
        assert_eq!(graph.swap_command_count, 1);
        assert_eq!(graph.passes[0].graph_command_index, 0);
        assert_eq!(graph.copies[0].graph_command_index, 1);
        assert_eq!(graph.swaps[0].graph_command_index, 2);
        assert_eq!(
            graph.passes[0].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(1))
        );
        assert_eq!(
            graph.passes[0].input_bindings[0].source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(3))
        );
        assert_eq!(
            graph.passes[0].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::NamedFbo(2))
        );
        assert_eq!(graph.swaps[0].a, SceneGraphTarget::NamedFbo(2));
        assert_eq!(graph.swaps[0].b, SceneGraphTarget::NamedFbo(1));
        assert_eq!(graph.copies[0].source, SceneGraphTarget::NamedFbo(2));
        assert_eq!(graph.copies[0].target, SceneGraphTarget::NamedFbo(1));
    }

    #[test]
    fn effect_pass_graph_applies_swap_alias_to_later_named_fbo_resolution() {
        let object = object(SceneObjectId(7), Some(SceneResourceId(11)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/fluidsimulation/effect.json".to_owned(),
                effect: WeEffectKind::Unknown,
                fbos: vec![
                    fbo("_rt_SmokeVelocity1", SceneGraphTarget::NamedFbo(1)),
                    fbo("_rt_SmokeVelocity2", SceneGraphTarget::NamedFbo(2)),
                ],
                commands: vec![
                    SceneEffectCommand::MaterialPass(super::super::SceneEffectMaterialPass {
                        pass_index: 1,
                        shader: Some("effects/fluidsimulation_advection".to_owned()),
                        source: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity1".to_owned(),
                        )),
                        target: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity2".to_owned(),
                        )),
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    }),
                    SceneEffectCommand::Swap(super::super::SceneEffectSwapCommand {
                        pass_index: 2,
                        a: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity2".to_owned()),
                        b: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity1".to_owned()),
                    }),
                    SceneEffectCommand::MaterialPass(super::super::SceneEffectMaterialPass {
                        pass_index: 3,
                        shader: Some("effects/fluidsimulation_divergence".to_owned()),
                        source: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity2".to_owned(),
                        )),
                        target: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_SmokeVelocity1".to_owned(),
                        )),
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    }),
                ],
            },
        }];

        let graph =
            SceneEffectPassGraphPlan::from_scene(&[object], &effects).expect("effect graph");

        assert_eq!(graph.passes[0].graph_command_index, 0);
        assert_eq!(graph.swaps[0].graph_command_index, 1);
        assert_eq!(graph.passes[1].graph_command_index, 2);
        assert_eq!(
            graph.passes[1].source.as_ref().unwrap().source,
            SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(1))
        );
        assert_eq!(
            graph.passes[1].output,
            SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::NamedFbo(2))
        );
    }

    #[test]
    fn effect_pass_graph_rejects_undeclared_named_fbo() {
        let object = object(SceneObjectId(7), Some(SceneResourceId(11)));
        let effects = vec![SceneObjectEffectProgram {
            object: object.id,
            program: super::super::SceneEffectProgram {
                effect_file: "effects/blur/effect.json".to_owned(),
                effect: WeEffectKind::Unknown,
                fbos: Vec::new(),
                commands: vec![SceneEffectCommand::MaterialPass(
                    super::super::SceneEffectMaterialPass {
                        pass_index: 0,
                        shader: Some("effects/blur_downsample4".to_owned()),
                        source: Some(SceneEffectImageRef::SourceTexture),
                        target: Some(SceneEffectImageRef::NamedFbo(
                            "_rt_QuarterCompoBuffer1".to_owned(),
                        )),
                        blend: SceneEffectPassBlend::NormalReplace,
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                        alpha_write: SceneAlphaWriteMode::Default,
                        texture_resources: Vec::new(),
                        binds: BTreeMap::new(),
                        combos: BTreeMap::new(),
                        constants: BTreeMap::new(),
                    },
                )],
            },
        }];

        let err = SceneEffectPassGraphPlan::from_scene(&[object], &effects)
            .expect_err("undeclared FBO must fail");

        assert!(err.contains("undeclared FBO"));
    }

    fn object(id: SceneObjectId, source: Option<SceneResourceId>) -> SceneObject {
        SceneObject {
            id,
            geometry: SceneObjectGeometry::Mesh {
                geometry: crate::engine::scene_engine::SceneGeometryId(1),
                vertex_count: 4,
                index_count: 6,
            },
            material: SceneMaterialContract {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: SceneMaterialRenderState::translucent_2d(),
            },
            source,
        }
    }

    fn fbo(name: &str, target: SceneGraphTarget) -> SceneEffectFboBinding {
        SceneEffectFboBinding {
            name: name.to_owned(),
            target,
            format: Some(SceneEffectFboFormat::Rgba16Float),
            scale: 1.0,
            unique: false,
        }
    }
}
