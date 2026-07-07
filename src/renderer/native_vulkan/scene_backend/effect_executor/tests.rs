use std::collections::BTreeMap;

use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
    SceneEffectPassGraphCopy, SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneEffectPassGraphPlan,
    SceneEffectPassGraphSwap, SceneEffectTextureResourceBinding, SceneGraphTarget, SceneObjectId,
    SceneResourceId, we::WeEffectKind,
};

use super::NativeVulkanSceneEffectRuntimeCommandSequencePlan;

#[test]
fn effect_runtime_command_sequence_orders_material_copy_and_swap() {
    let graph = SceneEffectPassGraphPlan {
        material_pass_count: 2,
        copy_command_count: 1,
        swap_command_count: 1,
        passes: vec![pass(0, 0), pass(3, 1)],
        copies: vec![SceneEffectPassGraphCopy {
            graph_command_index: 1,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 1,
            source: SceneGraphTarget::NamedFbo(1),
            target: SceneGraphTarget::NamedFbo(2),
        }],
        swaps: vec![SceneEffectPassGraphSwap {
            graph_command_index: 2,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 2,
            a: SceneGraphTarget::NamedFbo(2),
            b: SceneGraphTarget::NamedFbo(1),
        }],
        ..SceneEffectPassGraphPlan::empty()
    };

    let sequence =
        NativeVulkanSceneEffectRuntimeCommandSequencePlan::from_effect_pass_graph(&graph)
            .expect("effect command sequence");

    assert_eq!(
        sequence
            .entries
            .iter()
            .map(|entry| entry.kind)
            .collect::<Vec<_>>(),
        vec!["material", "copy", "swap", "material"]
    );
    assert_eq!(
        sequence.command_order,
        [
            "merge_effect_material_copy_swap_commands",
            "sort_by_scene_effect_graph_command_index",
            "validate_dense_effect_command_stream"
        ]
    );
}

#[test]
fn effect_runtime_command_sequence_rejects_sparse_indices() {
    let graph = SceneEffectPassGraphPlan {
        material_pass_count: 1,
        passes: vec![pass(2, 0)],
        ..SceneEffectPassGraphPlan::empty()
    };

    let err = NativeVulkanSceneEffectRuntimeCommandSequencePlan::from_effect_pass_graph(&graph)
        .expect_err("sparse command stream must fail");

    assert!(err.contains("dense and ordered"));
}

fn pass(graph_command_index: usize, graph_pass_index: usize) -> SceneEffectPassGraphMaterialPass {
    SceneEffectPassGraphMaterialPass {
        graph_command_index,
        graph_pass_index,
        object: SceneObjectId(7),
        program_index: 0,
        pass_index: graph_command_index,
        effect_file: "effects/test/effect.json".to_owned(),
        effect: WeEffectKind::Unknown,
        shader: Some("effects/iris".to_owned()),
        source: Some(SceneEffectPassGraphInputBinding {
            slot: 0,
            image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
            source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(9)),
        }),
        input_bindings: Vec::new(),
        output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::NamedFbo(4)),
        blend: SceneEffectPassBlend::NormalReplace,
        depth_test: SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: SceneCullMode::None,
        alpha_write: SceneAlphaWriteMode::Default,
        texture_resources: vec![SceneEffectTextureResourceBinding {
            slot: 1,
            resource: SceneResourceId(10),
        }],
        combos: BTreeMap::new(),
        constants: BTreeMap::new(),
    }
}
