use super::{
    SCENE_BINARY_NONE_ID, SceneBinaryError, SceneNode, SceneNodeKind, read_f32, read_u16, read_u32,
    write_f32, write_u16, write_u32,
};

pub const SCENE_BINARY_GEOMETRY_RECORD_SIZE: usize = 72;
pub const SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE: usize = 20;
pub const SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE: usize = 4;

pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD: u16 = 1;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_ELLIPSE: u16 = 2;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_TEXT: u16 = 3;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_PATH: u16 = 4;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES: u16 = 5;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_AUDIO_RESPONSE: u16 = 6;
pub const SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH: u16 = 7;

pub const SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED: u16 = 1;
pub const SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY: u16 = 2;

pub const SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT: u32 = 4;
pub const SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryGeometryRecord {
    pub owner_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub width: f32,
    pub height: f32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub material_uv_count: u32,
    pub primitive_kind: u16,
    pub vertex_layout: u16,
    pub bounds_min_x: f32,
    pub bounds_min_y: f32,
    pub bounds_max_x: f32,
    pub bounds_max_y: f32,
    pub uv_min_u: f32,
    pub uv_min_v: f32,
    pub uv_max_u: f32,
    pub uv_max_v: f32,
}

impl SceneBinaryGeometryRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_f32(out, self.width);
        write_f32(out, self.height);
        write_u32(out, self.first_vertex);
        write_u32(out, self.vertex_count);
        write_u32(out, self.first_index);
        write_u32(out, self.index_count);
        write_u32(out, self.material_uv_count);
        write_u16(out, self.primitive_kind);
        write_u16(out, self.vertex_layout);
        write_f32(out, self.bounds_min_x);
        write_f32(out, self.bounds_min_y);
        write_f32(out, self.bounds_max_x);
        write_f32(out, self.bounds_max_y);
        write_f32(out, self.uv_min_u);
        write_f32(out, self.uv_min_v);
        write_f32(out, self.uv_max_u);
        write_f32(out, self.uv_max_v);
        debug_assert_eq!(SCENE_BINARY_GEOMETRY_RECORD_SIZE, 72);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryGeometryVertexRecord {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    pub opacity: f32,
}

impl SceneBinaryGeometryVertexRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_f32(out, self.x);
        write_f32(out, self.y);
        write_f32(out, self.u);
        write_f32(out, self.v);
        write_f32(out, self.opacity);
        debug_assert_eq!(SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, 20);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryGeometryIndexRecord {
    pub index: u32,
}

impl SceneBinaryGeometryIndexRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.index);
        debug_assert_eq!(SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, 4);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneBinaryGeometryRanges {
    pub bounds_min_x: f32,
    pub bounds_min_y: f32,
    pub bounds_max_x: f32,
    pub bounds_max_y: f32,
    pub uv_min_u: f32,
    pub uv_min_v: f32,
    pub uv_max_u: f32,
    pub uv_max_v: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneBinaryGeometryStreamShape {
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub primitive_kind: u16,
    pub vertex_layout: u16,
}

pub(super) fn geometry_stream_shape(
    node: &SceneNode,
    first_mesh_vertex: u32,
    mesh_vertex_count: u32,
    first_mesh_index: u32,
    mesh_index_count: u32,
) -> SceneBinaryGeometryStreamShape {
    if node.mesh.is_some() {
        return SceneBinaryGeometryStreamShape {
            first_vertex: first_mesh_vertex,
            vertex_count: mesh_vertex_count,
            first_index: first_mesh_index,
            index_count: mesh_index_count,
            primitive_kind: SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH,
            vertex_layout: SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
        };
    }

    let (vertex_count, index_count) = generated_geometry_counts(node.kind);
    SceneBinaryGeometryStreamShape {
        first_vertex: SCENE_BINARY_NONE_ID,
        vertex_count,
        first_index: SCENE_BINARY_NONE_ID,
        index_count,
        primitive_kind: geometry_primitive_kind(node.kind),
        vertex_layout: SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED,
    }
}

pub(super) fn geometry_flags(node: &SceneNode) -> u16 {
    u16::from(node.width.is_some())
        | (u16::from(node.height.is_some()) << 1)
        | (u16::from(node.mesh.is_some()) << 2)
        | (u16::from(node.path_data.is_some()) << 3)
        | (u16::from(node.text.is_some()) << 4)
        | (u16::from(geometry_has_bounds(node)) << 5)
        | (u16::from(geometry_has_uv(node)) << 6)
}

pub(super) fn geometry_ranges(node: &SceneNode) -> SceneBinaryGeometryRanges {
    if let Some(mesh) = node.mesh.as_ref() {
        return mesh_geometry_ranges(mesh);
    }
    let width = node.width.unwrap_or(0.0) as f32;
    let height = node.height.unwrap_or(0.0) as f32;
    let uv_max = if geometry_has_uv(node) { 1.0 } else { 0.0 };
    SceneBinaryGeometryRanges {
        bounds_min_x: 0.0,
        bounds_min_y: 0.0,
        bounds_max_x: width,
        bounds_max_y: height,
        uv_min_u: 0.0,
        uv_min_v: 0.0,
        uv_max_u: uv_max,
        uv_max_v: uv_max,
    }
}

pub(super) fn geometry_has_uv(node: &SceneNode) -> bool {
    node.mesh.is_some() || matches!(node.kind, SceneNodeKind::Image | SceneNodeKind::Video)
}

pub(super) fn node_has_geometry(node: &SceneNode) -> bool {
    matches!(
        node.kind,
        SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::ParticleEmitter
            | SceneNodeKind::AudioResponse
    )
}

pub(crate) fn decode_geometry_record(
    bytes: &[u8],
) -> Result<SceneBinaryGeometryRecord, SceneBinaryError> {
    Ok(SceneBinaryGeometryRecord {
        owner_name: read_u32(bytes, 0)?,
        kind: read_u16(bytes, 4)?,
        flags: read_u16(bytes, 6)?,
        width: read_f32(bytes, 8)?,
        height: read_f32(bytes, 12)?,
        first_vertex: read_u32(bytes, 16)?,
        vertex_count: read_u32(bytes, 20)?,
        first_index: read_u32(bytes, 24)?,
        index_count: read_u32(bytes, 28)?,
        material_uv_count: read_u32(bytes, 32)?,
        primitive_kind: read_u16(bytes, 36)?,
        vertex_layout: read_u16(bytes, 38)?,
        bounds_min_x: read_f32(bytes, 40)?,
        bounds_min_y: read_f32(bytes, 44)?,
        bounds_max_x: read_f32(bytes, 48)?,
        bounds_max_y: read_f32(bytes, 52)?,
        uv_min_u: read_f32(bytes, 56)?,
        uv_min_v: read_f32(bytes, 60)?,
        uv_max_u: read_f32(bytes, 64)?,
        uv_max_v: read_f32(bytes, 68)?,
    })
}

pub(crate) fn decode_geometry_vertex_record(
    bytes: &[u8],
) -> Result<SceneBinaryGeometryVertexRecord, SceneBinaryError> {
    Ok(SceneBinaryGeometryVertexRecord {
        x: read_f32(bytes, 0)?,
        y: read_f32(bytes, 4)?,
        u: read_f32(bytes, 8)?,
        v: read_f32(bytes, 12)?,
        opacity: read_f32(bytes, 16)?,
    })
}

pub(crate) fn decode_geometry_index_record(
    bytes: &[u8],
) -> Result<SceneBinaryGeometryIndexRecord, SceneBinaryError> {
    Ok(SceneBinaryGeometryIndexRecord {
        index: read_u32(bytes, 0)?,
    })
}

fn mesh_geometry_ranges(mesh: &super::super::SceneMesh) -> SceneBinaryGeometryRanges {
    let Some(first) = mesh.vertices.first() else {
        return SceneBinaryGeometryRanges {
            bounds_min_x: 0.0,
            bounds_min_y: 0.0,
            bounds_max_x: 0.0,
            bounds_max_y: 0.0,
            uv_min_u: 0.0,
            uv_min_v: 0.0,
            uv_max_u: 0.0,
            uv_max_v: 0.0,
        };
    };
    let mut ranges = SceneBinaryGeometryRanges {
        bounds_min_x: first.x as f32,
        bounds_min_y: first.y as f32,
        bounds_max_x: first.x as f32,
        bounds_max_y: first.y as f32,
        uv_min_u: first.u as f32,
        uv_min_v: first.v as f32,
        uv_max_u: first.u as f32,
        uv_max_v: first.v as f32,
    };
    for vertex in &mesh.vertices[1..] {
        let x = vertex.x as f32;
        let y = vertex.y as f32;
        let u = vertex.u as f32;
        let v = vertex.v as f32;
        ranges.bounds_min_x = ranges.bounds_min_x.min(x);
        ranges.bounds_min_y = ranges.bounds_min_y.min(y);
        ranges.bounds_max_x = ranges.bounds_max_x.max(x);
        ranges.bounds_max_y = ranges.bounds_max_y.max(y);
        ranges.uv_min_u = ranges.uv_min_u.min(u);
        ranges.uv_min_v = ranges.uv_min_v.min(v);
        ranges.uv_max_u = ranges.uv_max_u.max(u);
        ranges.uv_max_v = ranges.uv_max_v.max(v);
    }
    ranges
}

fn geometry_has_bounds(node: &SceneNode) -> bool {
    node.mesh.is_some() || node.width.is_some() || node.height.is_some()
}

fn geometry_primitive_kind(kind: SceneNodeKind) -> u16 {
    match kind {
        SceneNodeKind::Image
        | SceneNodeKind::Video
        | SceneNodeKind::Color
        | SceneNodeKind::Rectangle => SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD,
        SceneNodeKind::Ellipse => SCENE_BINARY_GEOMETRY_PRIMITIVE_ELLIPSE,
        SceneNodeKind::Text => SCENE_BINARY_GEOMETRY_PRIMITIVE_TEXT,
        SceneNodeKind::Path => SCENE_BINARY_GEOMETRY_PRIMITIVE_PATH,
        SceneNodeKind::ParticleEmitter => SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES,
        SceneNodeKind::AudioResponse => SCENE_BINARY_GEOMETRY_PRIMITIVE_AUDIO_RESPONSE,
        SceneNodeKind::Group
        | SceneNodeKind::Shader
        | SceneNodeKind::Audio
        | SceneNodeKind::Script
        | SceneNodeKind::Unknown => unreachable!("node without binary geometry primitive"),
    }
}

fn generated_geometry_counts(kind: SceneNodeKind) -> (u32, u32) {
    match kind {
        SceneNodeKind::Image
        | SceneNodeKind::Video
        | SceneNodeKind::Color
        | SceneNodeKind::Rectangle => (
            SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT,
            SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT,
        ),
        SceneNodeKind::Ellipse
        | SceneNodeKind::Text
        | SceneNodeKind::Path
        | SceneNodeKind::ParticleEmitter
        | SceneNodeKind::AudioResponse => (0, 0),
        SceneNodeKind::Group
        | SceneNodeKind::Shader
        | SceneNodeKind::Audio
        | SceneNodeKind::Script
        | SceneNodeKind::Unknown => unreachable!("node without generated binary geometry"),
    }
}
