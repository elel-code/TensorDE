//! Backend-neutral effect-target planning.

use super::*;

pub(super) fn target_spec(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    allocation: SceneRenderingDeviceTargetAllocation,
    swapchain_format: TextureFormat,
    swapchain_extent: Extent2D,
) -> Result<SceneEffectTargetImagePlan, String> {
    let image_target =
        storage.document().image_targets.iter().find(|target| {
            target.name == allocation.target_name && target.role == allocation.target
        });
    let format = image_target
        .and_then(|target| storage.string(target.format))
        .map(|format| target_format(format, swapchain_format))
        .transpose()?
        .unwrap_or(swapchain_format);
    let extent = logical_target_extent(storage, allocation, swapchain_extent)?;
    Ok(SceneEffectTargetImagePlan {
        physical_slot: allocation.physical_slot,
        graph_index: allocation.graph_index,
        target: allocation.target,
        target_name: allocation.target_name,
        format,
        extent,
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames: matches!(
            allocation.target,
            SceneRenderTargetKind::NamedFbo | SceneRenderTargetKind::FirstClassEffectTarget
        ),
        aliased_logical_target_count: 1,
        input_attachment_required: graph.sampled_bindings.iter().any(|binding| {
            binding.access == SceneRenderingDeviceImageAccess::InputAttachment
                && binding.logical_target()
                    == Some((
                        allocation.graph_index,
                        allocation.target,
                        allocation.target_name,
                    ))
        }),
    })
}

pub(in super::super) fn logical_target_extent(
    storage: &SceneStorage,
    allocation: SceneRenderingDeviceTargetAllocation,
    swapchain_extent: Extent2D,
) -> Result<Extent2D, String> {
    let image_target =
        storage.document().image_targets.iter().find(|target| {
            target.name == allocation.target_name && target.role == allocation.target
        });
    let (width, height) = match allocation.extent_domain {
        SceneTargetExtentDomain::GraphSource => {
            return Err(format!(
                "scene target graph {} {:?}:{:?} retained an unresolved graph-source extent domain",
                allocation.graph_index, allocation.target, allocation.target_name
            ));
        }
        // The graph already resolved the owner dimensions and WE divisor on
        // the cold path. A physical fallback here would silently collapse an
        // authored target into the presentation domain.
        SceneTargetExtentDomain::OwnerAuthored => {
            if allocation.width == 0 || allocation.height == 0 {
                return Err(format!(
                    "owner-authored scene target graph {} {:?}:{:?} has no resolved extent",
                    allocation.graph_index, allocation.target, allocation.target_name
                ));
            }
            (allocation.width, allocation.height)
        }
        SceneTargetExtentDomain::PhysicalSurface => image_target
            .map(|target| scaled_extent(swapchain_extent, target))
            .unwrap_or((
                swapchain_extent.width.max(1),
                swapchain_extent.height.max(1),
            )),
    };
    Ok(Extent2D::new(width, height))
}

impl LogicalEffectTargetKey {
    pub(super) fn from_pass_target(pass: &SceneRenderingDevicePassNode) -> Option<Self> {
        Self::from_target(pass.graph_index, pass.target, pass.target_name)
    }

    pub(super) fn from_target(
        graph_index: u32,
        target: SceneRenderTargetKind,
        name: SceneStringId,
    ) -> Option<Self> {
        effect_target_kind_is_recordable(target).then_some(Self {
            graph_index,
            target,
            name,
        })
    }
}

pub(super) fn logical_target_references(
    allocations: &[SceneRenderingDeviceTargetAllocation],
) -> Vec<LogicalEffectTargetReference> {
    allocations
        .iter()
        .filter_map(|allocation| {
            LogicalEffectTargetKey::from_target(
                allocation.graph_index,
                allocation.target,
                allocation.target_name,
            )
            .map(|key| LogicalEffectTargetReference {
                key,
                physical_slot: allocation.physical_slot,
            })
        })
        .collect()
}

pub(super) fn swap_logical_references(
    command: SceneEffectTargetCommand,
    references: &mut [LogicalEffectTargetReference],
) -> Result<(), String> {
    let source_key = command
        .source
        .and_then(|source| match source {
            SceneEffectTargetCommandSource::LogicalTarget(key) => Some(key),
            SceneEffectTargetCommandSource::SceneColor => None,
        })
        .ok_or_else(|| "scene effect swap command has no logical source target".to_owned())?;
    let source_index = references
        .iter()
        .position(|reference| reference.key == source_key)
        .ok_or_else(|| "scene effect swap command source target is not allocated".to_owned())?;
    let target_index = references
        .iter()
        .position(|reference| reference.key == command.target)
        .ok_or_else(|| "scene effect swap command target is not allocated".to_owned())?;
    references.swap(source_index, target_index);
    references[source_index].key = source_key;
    references[target_index].key = command.target;
    Ok(())
}

pub(super) fn local_read_scope_matches_command(
    scope: &super::super::local_read::SceneLocalReadScopePlan,
    command: &SceneEffectTargetCommand,
    producer: bool,
) -> bool {
    let (pass_record_index, draw_range, target) = if producer {
        (
            scope.producer_pass_record_index(),
            scope.producer_draw_range(),
            scope.source(),
        )
    } else {
        (
            scope.consumer_pass_record_index(),
            scope.consumer_draw_range(),
            scope.destination(),
        )
    };
    command.kind == SceneEffectTargetCommandKind::DynamicRender
        && command.pass_record_index == pass_record_index
        && (command.mesh_draw_start, command.mesh_draw_count) == draw_range
        && command.target.graph_index == target.graph_index()
        && command.target.target == target.target()
        && command.target.name == target.target_name()
}

pub(super) fn command_source_key(
    storage: &SceneStorage,
    pass: &SceneRenderingDevicePassNode,
) -> Option<SceneEffectTargetCommandSource> {
    let start = pass.binding_start as usize;
    let end = start.saturating_add(pass.binding_count as usize);
    storage
        .document()
        .render_bindings
        .get(start..end)?
        .iter()
        .find_map(|binding| {
            if binding.target == SceneRenderTargetKind::SceneColor {
                Some(SceneEffectTargetCommandSource::SceneColor)
            } else {
                LogicalEffectTargetKey::from_target(pass.graph_index, binding.target, binding.name)
                    .map(SceneEffectTargetCommandSource::LogicalTarget)
            }
        })
}

pub(super) fn effect_target_kind_is_recordable(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::NamedFbo
            | SceneRenderTargetKind::FirstClassEffectTarget
            | SceneRenderTargetKind::Temporary
    )
}

pub(super) fn target_format(
    format: &str,
    swapchain_format: TextureFormat,
) -> Result<TextureFormat, String> {
    match format {
        "r8" | "r8_unorm" => Ok(TextureFormat::R8Unorm),
        "r16f" | "r16_float" => Ok(TextureFormat::R16Float),
        "rg1616f" | "rg16f" | "rg16_float" => Ok(TextureFormat::Rg16Float),
        "rgba8" | "rgba8_unorm" | "rgba8888" | "rgba" => Ok(TextureFormat::Rgba8Unorm),
        "rgba16f" | "rgba16_float" | "rgba16161616f" => Ok(TextureFormat::Rgba16Float),
        "rgba_backbuffer" | "rgb_backbuffer" | "" => Ok(swapchain_format),
        _ => Err(format!(
            "scene effect target format {format:?} is not supported by the renderer format map"
        )),
    }
}

pub(super) fn scaled_extent(extent: Extent2D, target: &SceneImageTargetRecord) -> (u32, u32) {
    let [width, height] = target.scaled_extent([extent.width, extent.height]);
    (width, height)
}

/// Matches WE's unsigned integer floor division for source-target scaling.
pub(super) fn divided_axis(value: u32, divisor_milli: u32) -> u32 {
    scene_target_scaled_axis(value, divisor_milli)
}
