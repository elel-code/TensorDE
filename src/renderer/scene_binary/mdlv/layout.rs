//! MDLV entry layout key facts.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

const MDLV_LAYOUT_ATTRIBUTES: &[(u32, u32)] = &[
    (0x0000_0001, 12),
    (0x0000_0002, 12),
    (0x0000_0004, 16),
    (0x0000_0008, 8),
    (0x0000_0010, 12),
    (0x0080_0000, 16),
    (0x0100_0000, 16),
    (0x0200_0000, 12),
    (0x0001_0000, 16),
];

pub(super) fn mdlv_layout_stride_bytes(layout_key: u32) -> Option<u32> {
    let mut stride = 0u32;
    let mut remaining = layout_key;
    for (bit, bytes) in MDLV_LAYOUT_ATTRIBUTES {
        if layout_key & bit != 0 {
            stride = stride.checked_add(*bytes)?;
            remaining &= !bit;
        }
    }
    (remaining == 0).then_some(stride)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdlv_layout_stride_matches_recovered_eye_layout() {
        assert_eq!(mdlv_layout_stride_bytes(0x0180_000f), Some(80));
        assert_eq!(mdlv_layout_stride_bytes(0x0000_0009), Some(20));
        assert_eq!(mdlv_layout_stride_bytes(0x8000_0000), None);
    }
}
