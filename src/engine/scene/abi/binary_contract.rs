//! Scene binary compatibility and fixed-size texture metadata contracts.

pub const SCENE_BINARY_MAGIC: [u8; 8] = *b"GSCNENG1";
pub const SCENE_BINARY_VERSION: u32 = 28;
pub const SCENE_BINARY_MIN_READ_VERSION: u32 = SCENE_BINARY_VERSION;
pub const SCENE_BINARY_ENDIANNESS_LITTLE: u8 = 1;

pub const SCENE_FEATURE_DESCRIPTOR_HEAP: u64 = 1 << 0;
pub const SCENE_FEATURE_RENDER_GRAPH: u64 = 1 << 1;
pub const SCENE_FEATURE_EMBEDDED_PAYLOADS: u64 = 1 << 2;
pub const SCENE_FEATURE_WE_SEMANTICS: u64 = 1 << 3;
pub const SCENE_DEFAULT_FEATURE_FLAGS: u64 = SCENE_FEATURE_DESCRIPTOR_HEAP
    | SCENE_FEATURE_RENDER_GRAPH
    | SCENE_FEATURE_EMBEDDED_PAYLOADS
    | SCENE_FEATURE_WE_SEMANTICS;

pub const CHUNK_STRING_TABLE: u32 = u32::from_le_bytes(*b"STRS");
pub const CHUNK_PROJECT: u32 = u32::from_le_bytes(*b"PROJ");
pub const CHUNK_SCENE_OBJECT: u32 = u32::from_le_bytes(*b"OBJT");
pub const CHUNK_RESOURCE: u32 = u32::from_le_bytes(*b"RSRC");
pub const CHUNK_RESOURCE_PAYLOAD: u32 = u32::from_le_bytes(*b"PAYL");
pub const CHUNK_TEXTURE: u32 = u32::from_le_bytes(*b"TEXR");
pub const CHUNK_TEXTURE_MIP: u32 = u32::from_le_bytes(*b"TXMP");
pub const CHUNK_TEXTURE_PAYLOAD: u32 = u32::from_le_bytes(*b"TXPD");
pub const CHUNK_MATERIAL: u32 = u32::from_le_bytes(*b"MTRL");
pub const CHUNK_EFFECT: u32 = u32::from_le_bytes(*b"EFFT");
pub const CHUNK_TIMELINE: u32 = u32::from_le_bytes(*b"TMLN");
pub const CHUNK_MESH: u32 = u32::from_le_bytes(*b"MESH");
pub const CHUNK_PUPPET: u32 = u32::from_le_bytes(*b"PUPP");
pub const CHUNK_PARTICLE: u32 = u32::from_le_bytes(*b"PART");
pub const CHUNK_AUDIO: u32 = u32::from_le_bytes(*b"AUDO");
pub const CHUNK_SCRIPT_BINDING: u32 = u32::from_le_bytes(*b"SCRP");
pub const CHUNK_POINTER_BINDING: u32 = u32::from_le_bytes(*b"PNTR");
pub const CHUNK_USER_PROPERTY_BINDING: u32 = u32::from_le_bytes(*b"UBND");
pub const CHUNK_RENDER_GRAPH: u32 = u32::from_le_bytes(*b"RGRF");
pub const CHUNK_IMAGE_TARGET: u32 = u32::from_le_bytes(*b"IMGT");
pub const CHUNK_SHADER_CONTRACT: u32 = u32::from_le_bytes(*b"SHDR");

pub const REQUIRED_SCENE_CHUNKS: &[u32] = &[
    CHUNK_STRING_TABLE,
    CHUNK_PROJECT,
    CHUNK_SCENE_OBJECT,
    CHUNK_RESOURCE,
    CHUNK_RESOURCE_PAYLOAD,
    CHUNK_TEXTURE,
    CHUNK_TEXTURE_MIP,
    CHUNK_TEXTURE_PAYLOAD,
    CHUNK_MATERIAL,
    CHUNK_EFFECT,
    CHUNK_TIMELINE,
    CHUNK_MESH,
    CHUNK_PUPPET,
    CHUNK_PARTICLE,
    CHUNK_AUDIO,
    CHUNK_SCRIPT_BINDING,
    CHUNK_POINTER_BINDING,
    CHUNK_USER_PROPERTY_BINDING,
    CHUNK_RENDER_GRAPH,
    CHUNK_IMAGE_TARGET,
    CHUNK_SHADER_CONTRACT,
];

pub const INVALID_STRING_ID: u32 = u32::MAX;
pub const INVALID_RESOURCE_ID: u32 = u32::MAX;
pub const INVALID_OBJECT_ID: u32 = u32::MAX;
pub const INVALID_MATERIAL_ID: u32 = u32::MAX;
pub const INVALID_EFFECT_ID: u32 = u32::MAX;

pub const SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE: usize = 32;
pub const SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS: usize = 1;
