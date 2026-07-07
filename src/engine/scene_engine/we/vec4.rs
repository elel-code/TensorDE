//! WE `vec4` ABI facts.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `reverse-engineered/shaders/genericimage4.vert`
//! - `reverse-engineered/shaders/genericimage4.frag`

use serde::Serialize;

pub const WE_VEC4_LANES: usize = 4;
pub const WE_VEC4_BYTES: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WeVec4 {
    lanes: [f32; WE_VEC4_LANES],
}

impl WeVec4 {
    pub const ZERO: Self = Self::from_lanes([0.0, 0.0, 0.0, 0.0]);
    pub const ONE: Self = Self::from_lanes([1.0, 1.0, 1.0, 1.0]);

    pub const fn from_lanes(lanes: [f32; WE_VEC4_LANES]) -> Self {
        Self { lanes }
    }

    pub const fn lanes(self) -> [f32; WE_VEC4_LANES] {
        self.lanes
    }

    pub const fn as_lanes(&self) -> &[f32; WE_VEC4_LANES] {
        &self.lanes
    }

    pub fn first_non_finite_lane(&self) -> Option<usize> {
        self.lanes.iter().position(|value| !value.is_finite())
    }

    pub fn write_le_bytes(self, bytes: &mut Vec<u8>) {
        bytes.reserve(WE_VEC4_BYTES as usize);
        for value in self.lanes {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

impl From<[f32; WE_VEC4_LANES]> for WeVec4 {
    fn from(lanes: [f32; WE_VEC4_LANES]) -> Self {
        Self::from_lanes(lanes)
    }
}

impl From<WeVec4> for [f32; WE_VEC4_LANES] {
    fn from(value: WeVec4) -> Self {
        value.lanes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn we_vec4_is_fixed_four_lane_sixteen_byte_abi() {
        let value = WeVec4::from_lanes([1.0, 2.0, 3.0, 4.0]);
        let mut bytes = Vec::new();

        value.write_le_bytes(&mut bytes);

        assert_eq!(WE_VEC4_LANES, 4);
        assert_eq!(WE_VEC4_BYTES, 16);
        assert_eq!(bytes.len(), WE_VEC4_BYTES as usize);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[12..16], &4.0f32.to_le_bytes());
    }

    #[test]
    fn we_vec4_tracks_non_finite_lane_index() {
        let value = WeVec4::from_lanes([1.0, 2.0, f32::NAN, 4.0]);

        assert_eq!(value.first_non_finite_lane(), Some(2));
    }
}
