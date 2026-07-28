use super::*;

/// Retained logical-to-physical state while one authored graph is recorded.
///
/// A graph may interleave scene-color passes with effect-target commands.  The
/// state therefore has to survive each command slice; rebuilding it from the
/// frame's initial permutation after every pass would silently undo authored
/// swap-reference operations.
#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetExecutionState {
    pub(super) references: Vec<LogicalEffectTargetReference>,
    pub(super) initialized_physical_slots: Vec<u32>,
    pub(super) initialized_logical_targets: Vec<LogicalEffectTargetKey>,
}

impl SceneEffectTargetExecutionState {
    pub(in crate::renderer::native_vulkan) fn new(
        target_allocations: &[SceneRenderingDeviceTargetAllocation],
        initial_reference_physical_slots: &[u32],
        resources: &[SceneEffectTargetImageResource],
    ) -> Result<Self, String> {
        let mut references = logical_target_references(target_allocations);
        if references.len() != initial_reference_physical_slots.len() {
            return Err(format!(
                "scene effect target reference phase has {} slots for {} logical targets",
                initial_reference_physical_slots.len(),
                references.len()
            ));
        }
        for (reference, physical_slot) in references
            .iter_mut()
            .zip(initial_reference_physical_slots.iter().copied())
        {
            reference.physical_slot = physical_slot;
        }
        let initialized_physical_slots = resources
            .iter()
            .map(|resource| resource.plan.physical_slot)
            .collect();
        let initialized_logical_targets = references
            .iter()
            .filter(|reference| {
                resources.iter().any(|resource| {
                    resource.plan.physical_slot == reference.physical_slot
                        && resource.plan.persistent_across_frames
                })
            })
            .map(|reference| reference.key)
            .collect();
        Ok(Self {
            references,
            initialized_physical_slots,
            initialized_logical_targets,
        })
    }
}
