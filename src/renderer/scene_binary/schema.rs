//! Binary `.gscn` schema constants shared by ingest modules.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/effect-format.md`

pub(super) const BINARY_TRANSFORM_PROPERTY_DEFAULT: u16 = 0;
pub(super) const BINARY_TRANSFORM_PROPERTY_X: u16 = 1;
pub(super) const BINARY_TRANSFORM_PROPERTY_Y: u16 = 2;
pub(super) const BINARY_TRANSFORM_PROPERTY_SCALE_X: u16 = 3;
pub(super) const BINARY_TRANSFORM_PROPERTY_SCALE_Y: u16 = 4;
pub(super) const BINARY_TRANSFORM_PROPERTY_OPACITY: u16 = 5;
pub(super) const BINARY_TRANSFORM_PROPERTY_ROTATION_DEG: u16 = 6;
pub(super) const BINARY_TRANSFORM_PROPERTY_WIDTH: u16 = 7;
pub(super) const BINARY_TRANSFORM_PROPERTY_HEIGHT: u16 = 8;
pub(super) const BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS: u16 = 9;
pub(super) const BINARY_TRANSFORM_FLAG_LOOP: u16 = 1;

pub(super) const BINARY_NODE_FLAG_VISIBLE: u16 = 1;
pub(super) const BINARY_NODE_FLAG_COLOR: u16 = 1 << 7;
pub(super) const BINARY_NODE_FLAG_STROKE_COLOR: u16 = 1 << 8;
pub(super) const BINARY_NODE_FLAG_STROKE_WIDTH: u16 = 1 << 9;
pub(super) const BINARY_NODE_FLAG_CORNER_RADIUS: u16 = 1 << 10;

pub(super) const BINARY_EFFECT_UV_HAS_INPUT_EXTENT: u16 = 1;
pub(super) const BINARY_EFFECT_UV_HAS_MASK_EXTENT: u16 = 1 << 1;
pub(super) const BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT: u16 = 1 << 2;

pub(super) const BINARY_TEXTURE_ROLE_BASE_COLOR: u16 = 1;
