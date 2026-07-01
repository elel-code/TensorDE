use super::{SceneBinaryError, SceneEffect, read_u32, write_u32};

pub const SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE: usize = 32;

pub const SCENE_BINARY_MOTION_FAMILY_FLUTTER: u32 = 1;
pub const SCENE_BINARY_MOTION_FAMILY_SWAY: u32 = 2;
pub const SCENE_BINARY_MOTION_FAMILY_SHAKE: u32 = 4;
pub const SCENE_BINARY_MOTION_FAMILY_DRIFT: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryFlutterStateRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub first_parameter: u32,
    pub parameter_count: u32,
    pub pass_count: u32,
    pub motion_family_mask: u32,
    pub anchor_name: u32,
    pub dirty_range_count: u32,
}

impl SceneBinaryFlutterStateRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.first_parameter);
        write_u32(out, self.parameter_count);
        write_u32(out, self.pass_count);
        write_u32(out, self.motion_family_mask);
        write_u32(out, self.anchor_name);
        write_u32(out, self.dirty_range_count);
        debug_assert_eq!(SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE, 32);
    }
}

pub(super) fn effect_is_motion_family(effect: &SceneEffect) -> bool {
    motion_family_mask(effect) != 0
}

pub(super) fn motion_family_mask(effect: &SceneEffect) -> u32 {
    let file = effect.file.to_ascii_lowercase();
    let runtime = effect.runtime.as_deref().unwrap_or_default();
    u32::from(file.contains("flutter") || runtime.contains("flutter"))
        | (u32::from(file.contains("sway") || runtime.contains("sway")) << 1)
        | (u32::from(file.contains("shake") || runtime.contains("shake")) << 2)
        | (u32::from(file.contains("drift") || runtime.contains("drift")) << 3)
}

pub(super) fn motion_dirty_range_count(effect: &SceneEffect, parameter_count: u32) -> u32 {
    let mask = motion_family_mask(effect);
    let final_vertex_dirty = u32::from(mask != 0);
    let material_parameter_dirty = u32::from(parameter_count > 0);
    let pass_binding_dirty = u32::from(!effect.passes.is_empty());
    final_vertex_dirty
        .saturating_add(material_parameter_dirty)
        .saturating_add(pass_binding_dirty)
        .max(1)
}

pub(crate) fn decode_flutter_state_record(
    bytes: &[u8],
) -> Result<SceneBinaryFlutterStateRecord, SceneBinaryError> {
    Ok(SceneBinaryFlutterStateRecord {
        owner_name: read_u32(bytes, 0)?,
        effect_name: read_u32(bytes, 4)?,
        first_parameter: read_u32(bytes, 8)?,
        parameter_count: read_u32(bytes, 12)?,
        pass_count: read_u32(bytes, 16)?,
        motion_family_mask: read_u32(bytes, 20)?,
        anchor_name: read_u32(bytes, 24)?,
        dirty_range_count: read_u32(bytes, 28)?,
    })
}
