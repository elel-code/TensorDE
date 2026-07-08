use super::{
    SCENE_BINARY_NONE_ID, SceneBinaryError, read_f32, read_u16, read_u32, write_f32, write_u16,
    write_u32,
};

pub const SCENE_BINARY_NODE_RECORD_SIZE_V12: usize = 116;
pub const SCENE_BINARY_NODE_RECORD_SIZE: usize = 124;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryNodeRecord {
    pub id_name: u32,
    pub display_name: u32,
    pub parent_index: u32,
    pub resource_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub draw_order: u32,
    pub child_count: u32,
    pub first_child_index: u32,
    pub subtree_node_count: u32,
    pub effect_count: u32,
    pub audio_count: u32,
    pub property_count: u32,
    pub material_index: u32,
    pub geometry_index: u32,
    pub first_transform: u32,
    pub transform_count: u32,
    pub puppet_index: u32,
    pub particle_index: u32,
    pub puppet_attachment_name: u32,
    pub opacity: f32,
    pub color_rgba: u32,
    pub stroke_color_rgba: u32,
    pub stroke_width: f32,
    pub corner_radius: f32,
    pub font_size: f32,
    pub text_name: u32,
    pub font_family_name: u32,
    pub font_resource_name: u32,
    pub font_weight_name: u32,
    pub fit: u16,
    pub text_align: u16,
    pub puppet_source_name: u32,
}

impl SceneBinaryNodeRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.id_name);
        write_u32(out, self.display_name);
        write_u32(out, self.parent_index);
        write_u32(out, self.resource_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_u32(out, self.draw_order);
        write_u32(out, self.child_count);
        write_u32(out, self.first_child_index);
        write_u32(out, self.subtree_node_count);
        write_u32(out, self.effect_count);
        write_u32(out, self.audio_count);
        write_u32(out, self.property_count);
        write_u32(out, self.material_index);
        write_u32(out, self.geometry_index);
        write_u32(out, self.first_transform);
        write_u32(out, self.transform_count);
        write_u32(out, self.puppet_index);
        write_u32(out, self.particle_index);
        write_u32(out, self.puppet_attachment_name);
        write_f32(out, self.opacity);
        write_u32(out, self.color_rgba);
        write_u32(out, self.stroke_color_rgba);
        write_f32(out, self.stroke_width);
        write_f32(out, self.corner_radius);
        write_f32(out, self.font_size);
        write_u32(out, self.text_name);
        write_u32(out, self.font_family_name);
        write_u32(out, self.font_resource_name);
        write_u32(out, self.font_weight_name);
        write_u16(out, self.fit);
        write_u16(out, self.text_align);
        write_u32(out, self.puppet_source_name);
        debug_assert_eq!(SCENE_BINARY_NODE_RECORD_SIZE, 124);
    }
}

pub(crate) fn decode_node_record(bytes: &[u8]) -> Result<SceneBinaryNodeRecord, SceneBinaryError> {
    Ok(SceneBinaryNodeRecord {
        id_name: read_u32(bytes, 0)?,
        display_name: read_u32(bytes, 4)?,
        parent_index: read_u32(bytes, 8)?,
        resource_name: read_u32(bytes, 12)?,
        kind: read_u16(bytes, 16)?,
        flags: read_u16(bytes, 18)?,
        draw_order: read_u32(bytes, 20)?,
        child_count: read_u32(bytes, 24)?,
        first_child_index: read_u32(bytes, 28)?,
        subtree_node_count: read_u32(bytes, 32)?,
        effect_count: read_u32(bytes, 36)?,
        audio_count: read_u32(bytes, 40)?,
        property_count: read_u32(bytes, 44)?,
        material_index: read_u32(bytes, 48)?,
        geometry_index: read_u32(bytes, 52)?,
        first_transform: read_u32(bytes, 56)?,
        transform_count: read_u32(bytes, 60)?,
        puppet_index: read_u32(bytes, 64)?,
        particle_index: read_u32(bytes, 68)?,
        puppet_attachment_name: read_u32(bytes, 72)?,
        opacity: read_f32(bytes, 76)?,
        color_rgba: read_u32(bytes, 80)?,
        stroke_color_rgba: read_u32(bytes, 84)?,
        stroke_width: read_f32(bytes, 88)?,
        corner_radius: read_f32(bytes, 92)?,
        font_size: read_f32(bytes, 96)?,
        text_name: read_u32(bytes, 100)?,
        font_family_name: read_u32(bytes, 104)?,
        font_resource_name: read_u32(bytes, 108)?,
        font_weight_name: read_u32(bytes, 112)?,
        fit: read_u16_or(bytes, 116, 1)?,
        text_align: read_u16_or(bytes, 118, 0)?,
        puppet_source_name: read_u32_or(bytes, 120, SCENE_BINARY_NONE_ID)?,
    })
}

fn read_u16_or(bytes: &[u8], offset: usize, default: u16) -> Result<u16, SceneBinaryError> {
    if offset + 2 <= bytes.len() {
        read_u16(bytes, offset)
    } else {
        Ok(default)
    }
}

fn read_u32_or(bytes: &[u8], offset: usize, default: u32) -> Result<u32, SceneBinaryError> {
    if offset + 4 <= bytes.len() {
        read_u32(bytes, offset)
    } else {
        Ok(default)
    }
}
