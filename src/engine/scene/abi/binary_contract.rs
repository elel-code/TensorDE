//! Scene binary compatibility and fixed-size texture metadata contracts.

pub const SCENE_BINARY_MAGIC: [u8; 8] = *b"GSCNENG1";
pub const SCENE_BINARY_VERSION: u32 = 15;
pub const SCENE_BINARY_MIN_READ_VERSION: u32 = 9;
pub const SCENE_BINARY_ENDIANNESS_LITTLE: u8 = 1;

pub const SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE: usize = 32;
pub const SCENE_TEXTURE_ALPHA_COVERAGE_GUARD_CELLS: usize = 1;
