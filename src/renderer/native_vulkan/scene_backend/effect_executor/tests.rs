use std::collections::BTreeMap;

use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
    SceneEffectPassGraphCopy, SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneEffectPassGraphPlan,
    SceneEffectPassGraphSwap, SceneEffectTextureResourceBinding, SceneGraphTarget, SceneObjectId,
    SceneResourceId, we::WeEffectKind,
};

use super::{
    NativeVulkanSceneEffectObjectCommandKind, NativeVulkanSceneEffectRuntimeCommandSequencePlan,
    native_vulkan_plan_scene_effect_object_command_streams,
};

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

#[test]
fn effect_object_command_stream_partitions_contiguous_material_copy_swap_commands() {
    let graph = SceneEffectPassGraphPlan {
        material_pass_count: 2,
        copy_command_count: 1,
        swap_command_count: 1,
        passes: vec![
            pass_to(0, 0, SceneGraphTarget::NamedFbo(1)),
            pass_to(3, 1, SceneGraphTarget::ObjectFinal(SceneObjectId(7))),
        ],
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

    let streams =
        native_vulkan_plan_scene_effect_object_command_streams(&graph).expect("stream plan");

    assert_eq!(streams.stream_count, 1);
    assert_eq!(streams.command_count, 4);
    assert_eq!(streams.layer_final_pass_count, 1);
    assert_eq!(streams.streams[0].object, SceneObjectId(7));
    assert_eq!(streams.streams[0].command_count, 4);
    assert_eq!(streams.streams[0].layer_final_pass_count, 1);
    assert_eq!(
        streams
            .entries
            .iter()
            .map(|entry| entry.kind)
            .collect::<Vec<_>>(),
        vec![
            NativeVulkanSceneEffectObjectCommandKind::Material,
            NativeVulkanSceneEffectObjectCommandKind::Copy,
            NativeVulkanSceneEffectObjectCommandKind::Swap,
            NativeVulkanSceneEffectObjectCommandKind::Material,
        ]
    );
}

#[test]
fn effect_object_command_stream_counts_image_layer_final_source_as_layer_final() {
    let object = SceneObjectId(1530);
    let image_layer_target = crate::engine::scene_engine::SceneImageLayerTargetPlan::for_object(
        object,
        Some(SceneResourceId(9)),
        1,
    )
    .expect("image-layer target");
    let graph = SceneEffectPassGraphPlan {
        object_program_count: 1,
        material_pass_count: 1,
        image_layer_target_count: 1,
        image_layer_scene_output_pass_count: 1,
        image_layer_targets: vec![image_layer_target],
        passes: vec![SceneEffectPassGraphMaterialPass {
            object,
            ..pass_to(0, 0, SceneGraphTarget::ImageLayerCompositeA(object))
        }],
        ..SceneEffectPassGraphPlan::empty()
    };

    let streams =
        native_vulkan_plan_scene_effect_object_command_streams(&graph).expect("stream plan");

    assert_eq!(streams.stream_count, 1);
    assert_eq!(streams.layer_final_pass_count, 1);
    assert_eq!(streams.streams[0].layer_final_pass_count, 1);
}

#[test]
fn effect_object_command_stream_preserves_interleaved_object_chunks_for_compositor_gate() {
    let other = SceneObjectId(8);
    let mut second_object = pass_to(1, 1, SceneGraphTarget::ObjectFinal(other));
    second_object.object = other;
    let graph = SceneEffectPassGraphPlan {
        object_program_count: 2,
        material_pass_count: 3,
        passes: vec![
            pass_to(0, 0, SceneGraphTarget::NamedFbo(1)),
            second_object,
            pass_to(2, 2, SceneGraphTarget::ObjectFinal(SceneObjectId(7))),
        ],
        ..SceneEffectPassGraphPlan::empty()
    };

    let streams =
        native_vulkan_plan_scene_effect_object_command_streams(&graph).expect("stream plan");

    assert_eq!(
        streams
            .streams
            .iter()
            .map(|stream| stream.object)
            .collect::<Vec<_>>(),
        vec![SceneObjectId(7), SceneObjectId(8), SceneObjectId(7)]
    );
}

fn pass(graph_command_index: usize, graph_pass_index: usize) -> SceneEffectPassGraphMaterialPass {
    pass_to(
        graph_command_index,
        graph_pass_index,
        SceneGraphTarget::NamedFbo(4),
    )
}

fn pass_to(
    graph_command_index: usize,
    graph_pass_index: usize,
    output: SceneGraphTarget,
) -> SceneEffectPassGraphMaterialPass {
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
        output: match output {
            SceneGraphTarget::ObjectFinal(object) => {
                SceneEffectPassGraphOutput::ObjectFinal(object)
            }
            target => SceneEffectPassGraphOutput::GraphTarget(target),
        },
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
