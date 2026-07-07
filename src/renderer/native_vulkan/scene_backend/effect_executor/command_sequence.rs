//! Effect graph command stream ordering.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneEffectPassGraphCopy, SceneEffectPassGraphMaterialPass, SceneEffectPassGraphPlan,
    SceneEffectPassGraphSwap, SceneObjectId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimeCommandSequencePlan {
    pub command_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub entries: Vec<NativeVulkanSceneEffectRuntimeCommandSequenceEntry>,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimeCommandSequenceEntry {
    pub graph_command_index: usize,
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub kind: &'static str,
}

pub(super) enum SceneEffectGraphCommand<'a> {
    Material(&'a SceneEffectPassGraphMaterialPass),
    Copy(&'a SceneEffectPassGraphCopy),
    Swap(&'a SceneEffectPassGraphSwap),
}

impl NativeVulkanSceneEffectRuntimeCommandSequencePlan {
    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph(
        graph: &SceneEffectPassGraphPlan,
    ) -> Result<Self, String> {
        let commands = ordered_effect_graph_commands(graph)?;
        let entries = commands
            .iter()
            .map(
                |command| NativeVulkanSceneEffectRuntimeCommandSequenceEntry {
                    graph_command_index: command.graph_command_index(),
                    effect_pass_index: command.pass_index(),
                    object: command.object(),
                    kind: command.kind(),
                },
            )
            .collect::<Vec<_>>();

        Ok(Self {
            command_count: entries.len(),
            material_pass_count: graph.material_pass_count,
            copy_command_count: graph.copy_command_count,
            swap_command_count: graph.swap_command_count,
            entries,
            command_order: [
                "merge_effect_material_copy_swap_commands",
                "sort_by_scene_effect_graph_command_index",
                "validate_dense_effect_command_stream",
            ],
        })
    }
}

pub(super) fn ordered_effect_graph_commands<'a>(
    graph: &'a SceneEffectPassGraphPlan,
) -> Result<Vec<SceneEffectGraphCommand<'a>>, String> {
    let mut commands = Vec::with_capacity(
        graph
            .material_pass_count
            .saturating_add(graph.copy_command_count)
            .saturating_add(graph.swap_command_count),
    );
    commands.extend(graph.passes.iter().map(SceneEffectGraphCommand::Material));
    commands.extend(graph.copies.iter().map(SceneEffectGraphCommand::Copy));
    commands.extend(graph.swaps.iter().map(SceneEffectGraphCommand::Swap));
    commands.sort_by_key(SceneEffectGraphCommand::graph_command_index);
    for (expected, command) in commands.iter().enumerate() {
        if command.graph_command_index() != expected {
            return Err(format!(
                "scene effect command stream must be dense and ordered; expected command index {expected}, got {} ({})",
                command.graph_command_index(),
                command.kind()
            ));
        }
    }
    Ok(commands)
}

impl SceneEffectGraphCommand<'_> {
    fn graph_command_index(&self) -> usize {
        match self {
            Self::Material(pass) => pass.graph_command_index,
            Self::Copy(copy) => copy.graph_command_index,
            Self::Swap(swap) => swap.graph_command_index,
        }
    }

    fn pass_index(&self) -> usize {
        match self {
            Self::Material(pass) => pass.pass_index,
            Self::Copy(copy) => copy.pass_index,
            Self::Swap(swap) => swap.pass_index,
        }
    }

    fn object(&self) -> SceneObjectId {
        match self {
            Self::Material(pass) => pass.object,
            Self::Copy(copy) => copy.object,
            Self::Swap(swap) => swap.object,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Material(_) => "material",
            Self::Copy(_) => "copy",
            Self::Swap(_) => "swap",
        }
    }
}
