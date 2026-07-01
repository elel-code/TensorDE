use super::super::{
    SCENE_PARTICLE_MAX_COUNT, SceneNode, SceneNodeKind, SceneParticleEmitterSettings,
    SceneTransform, scene_particle_unit,
};
use super::{
    SceneBinaryError, read_f32, read_u16, read_u32, read_u64, scene_binary_color_rgba, write_f32,
    write_u16, write_u32, write_u64,
};

pub const SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE: usize = 72;

pub const SCENE_BINARY_PARTICLE_FLAG_LOOP: u16 = 1;
pub const SCENE_BINARY_PARTICLE_FLAG_FADE: u16 = 1 << 1;

pub const SCENE_BINARY_PARTICLE_SHAPE_RECTANGLE: u16 = 1;
pub const SCENE_BINARY_PARTICLE_SHAPE_ELLIPSE: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryParticleEmitterRecord {
    pub owner_name: u32,
    pub count: u32,
    pub seed: u64,
    pub lifetime_ms: u64,
    pub spawn_width: f32,
    pub spawn_height: f32,
    pub particle_width: f32,
    pub particle_height: f32,
    pub speed_min: f32,
    pub speed_max: f32,
    pub direction_deg: f32,
    pub spread_deg: f32,
    pub gravity_x: f32,
    pub gravity_y: f32,
    pub color_rgba: u32,
    pub flags: u16,
    pub shape: u16,
}

impl SceneBinaryParticleEmitterRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.count);
        write_u64(out, self.seed);
        write_u64(out, self.lifetime_ms);
        write_f32(out, self.spawn_width);
        write_f32(out, self.spawn_height);
        write_f32(out, self.particle_width);
        write_f32(out, self.particle_height);
        write_f32(out, self.speed_min);
        write_f32(out, self.speed_max);
        write_f32(out, self.direction_deg);
        write_f32(out, self.spread_deg);
        write_f32(out, self.gravity_x);
        write_f32(out, self.gravity_y);
        write_u32(out, self.color_rgba);
        write_u16(out, self.flags);
        write_u16(out, self.shape);
        debug_assert_eq!(SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE, 72);
    }

    pub fn particle_count(self) -> u32 {
        self.count.min(SCENE_PARTICLE_MAX_COUNT)
    }

    pub fn opacity_and_transform_at(
        self,
        time_ms: u64,
        index: u32,
    ) -> Option<(f64, f64, f64, f64)> {
        let age = self.age_seconds(time_ms, index)?;
        let lifetime_ms = self.lifetime_ms.max(1);
        let progress = (age * 1000.0 / lifetime_ms as f64).clamp(0.0, 1.0);
        let opacity = if self.flags & SCENE_BINARY_PARTICLE_FLAG_FADE != 0 {
            1.0 - progress
        } else {
            1.0
        };
        let spawn_x =
            (scene_particle_unit(self.seed, index, 1) - 0.5) * f64::from(self.spawn_width);
        let spawn_y =
            (scene_particle_unit(self.seed, index, 2) - 0.5) * f64::from(self.spawn_height);
        let speed = f64::from(self.speed_min)
            + f64::from(self.speed_max - self.speed_min) * scene_particle_unit(self.seed, index, 3);
        let direction = f64::from(self.direction_deg)
            + (scene_particle_unit(self.seed, index, 4) - 0.5) * f64::from(self.spread_deg);
        let (direction_sin, direction_cos) = direction.to_radians().sin_cos();
        let x = spawn_x + direction_cos * speed * age + 0.5 * f64::from(self.gravity_x) * age * age;
        let y = spawn_y + direction_sin * speed * age + 0.5 * f64::from(self.gravity_y) * age * age;
        Some((opacity, x, y, direction))
    }

    fn age_seconds(self, time_ms: u64, index: u32) -> Option<f64> {
        let lifetime_ms = self.lifetime_ms.max(1);
        let phase = scene_particle_unit(self.seed, index, 0);
        let phase_ms = (phase * lifetime_ms as f64).round() as u64;
        let local_ms = if self.flags & SCENE_BINARY_PARTICLE_FLAG_LOOP != 0 {
            time_ms.wrapping_add(phase_ms) % lifetime_ms
        } else {
            let started_at = phase_ms.min(lifetime_ms);
            if time_ms < started_at {
                return None;
            }
            (time_ms - started_at).min(lifetime_ms)
        };
        Some(local_ms as f64 / 1000.0)
    }
}

pub(crate) fn decode_particle_emitter_record(
    bytes: &[u8],
) -> Result<SceneBinaryParticleEmitterRecord, SceneBinaryError> {
    Ok(SceneBinaryParticleEmitterRecord {
        owner_name: read_u32(bytes, 0)?,
        count: read_u32(bytes, 4)?,
        seed: read_u64(bytes, 8)?,
        lifetime_ms: read_u64(bytes, 16)?,
        spawn_width: read_f32(bytes, 24)?,
        spawn_height: read_f32(bytes, 28)?,
        particle_width: read_f32(bytes, 32)?,
        particle_height: read_f32(bytes, 36)?,
        speed_min: read_f32(bytes, 40)?,
        speed_max: read_f32(bytes, 44)?,
        direction_deg: read_f32(bytes, 48)?,
        spread_deg: read_f32(bytes, 52)?,
        gravity_x: read_f32(bytes, 56)?,
        gravity_y: read_f32(bytes, 60)?,
        color_rgba: read_u32(bytes, 64)?,
        flags: read_u16(bytes, 68)?,
        shape: read_u16(bytes, 70)?,
    })
}

pub fn scene_binary_particle_shape_kind(shape: u16) -> SceneNodeKind {
    match shape {
        SCENE_BINARY_PARTICLE_SHAPE_ELLIPSE => SceneNodeKind::Ellipse,
        _ => SceneNodeKind::Rectangle,
    }
}

pub fn scene_binary_particle_transform(
    parent: SceneTransform,
    parent_sin: f64,
    parent_cos: f64,
    x: f64,
    y: f64,
    rotation_deg: f64,
) -> SceneTransform {
    let child_x = x * parent.scale_x;
    let child_y = y * parent.scale_y;
    let rotated_child_x = child_x.mul_add(parent_cos, -child_y * parent_sin);
    let rotated_child_y = child_x.mul_add(parent_sin, child_y * parent_cos);
    SceneTransform {
        x: parent.x + rotated_child_x,
        y: parent.y + rotated_child_y,
        scale_x: parent.scale_x,
        scale_y: parent.scale_y,
        rotation_deg: parent.rotation_deg + rotation_deg,
        anchor_x: 0.5,
        anchor_y: 0.5,
    }
}

pub(super) fn particle_emitter_record_from_node(
    owner_name: u32,
    node: &SceneNode,
) -> Option<SceneBinaryParticleEmitterRecord> {
    if node.kind != SceneNodeKind::ParticleEmitter {
        return None;
    }
    let settings = SceneParticleEmitterSettings::from_node(node)?;
    Some(SceneBinaryParticleEmitterRecord {
        owner_name,
        count: settings.count.min(SCENE_PARTICLE_MAX_COUNT),
        seed: settings.seed,
        lifetime_ms: settings.lifetime_ms.max(1),
        spawn_width: settings.spawn_width as f32,
        spawn_height: settings.spawn_height as f32,
        particle_width: settings.particle_width as f32,
        particle_height: settings.particle_height as f32,
        speed_min: settings.speed_min as f32,
        speed_max: settings.speed_max as f32,
        direction_deg: settings.direction_deg as f32,
        spread_deg: settings.spread_deg as f32,
        gravity_x: settings.gravity_x as f32,
        gravity_y: settings.gravity_y as f32,
        color_rgba: scene_binary_color_rgba(Some(&settings.color)),
        flags: particle_emitter_flags(settings.loop_playback, settings.fade),
        shape: particle_shape_code(settings.shape),
    })
}

fn particle_emitter_flags(loop_playback: bool, fade: bool) -> u16 {
    u16::from(loop_playback) | (u16::from(fade) << 1)
}

fn particle_shape_code(kind: SceneNodeKind) -> u16 {
    match kind {
        SceneNodeKind::Ellipse => SCENE_BINARY_PARTICLE_SHAPE_ELLIPSE,
        _ => SCENE_BINARY_PARTICLE_SHAPE_RECTANGLE,
    }
}
