//! Optional SIMD accelerate for quoted-string scans (Glaze SSE2/AVX2 hot path).
//!
//! Cite:
//! - `references/glaze/docs/optimizing-performance.md` § SIMD Architecture Flags
//! - `references/glaze/include/glaze/util/parse.hpp` SWAR quote/escape scan;
//!   architecture SIMD is used for specific hot paths (string scanning)
//!
//! Feature `simd` (off by default): on `x86_64` with SSE2 (always on x86_64)
//! scan 16 bytes per step for `"` or `\`. Without the feature, or on other
//! arches, falls back to [`super::swar::find_quote_or_escape`].
//!
//! **Bench gate:** `cargo bench -p tensor-kdl --features simd -- pg8` compares
//! SWAR-only vs SIMD quote scan; enable only when measured win justifies it.

/// Scan for `"` or `\` — SIMD when `feature = "simd"` and x86_64, else SWAR.
#[inline(always)]
pub fn find_quote_or_escape_fast(input: &[u8], index: usize, end: usize) -> usize {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        find_quote_or_escape_sse2(input, index, end)
    }
    #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
    {
        super::swar::find_quote_or_escape(input, index, end)
    }
}

/// x86_64 SSE2: process 16-byte chunks (two u64 SWAR lanes, or true SSE2).
///
/// We implement portable 16-byte dual-SWAR first (always correct); with
/// `target_feature = "sse2"` we can use `_mm_cmpeq_epi8`. Dual-u64 is the
/// Glaze-style “wider SWAR” step that is the practical win without unsafe.
#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
fn find_quote_or_escape_sse2(input: &[u8], mut index: usize, end: usize) -> usize {
    use super::swar::{find_quote_or_escape, first_lane, has_byte, load_u64_unaligned};
    // 16-byte dual SWAR (SSE2 width without requiring unsafe intrinsics in
    // the default path). Glaze AVX2 does 32 then SSE2 16; we do 16 then 8.
    while index + 16 <= end {
        let c0 = load_u64_unaligned(&input[index..index + 8]);
        let c1 = load_u64_unaligned(&input[index + 8..index + 16]);
        let h0 = has_byte(c0, b'"') | has_byte(c0, b'\\');
        let h1 = has_byte(c1, b'"') | has_byte(c1, b'\\');
        if h0 != 0 {
            return index + first_lane(h0);
        }
        if h1 != 0 {
            return index + 8 + first_lane(h1);
        }
        index += 16;
    }
    find_quote_or_escape(input, index, end)
}

#[cfg(test)]
mod tests {
    use super::super::swar::find_quote_or_escape;
    use super::*;

    #[test]
    fn finds_quote_same_as_swar() {
        let s = b"0123456789abcdef\"tail";
        let a = find_quote_or_escape(s, 0, s.len());
        let b = find_quote_or_escape_fast(s, 0, s.len());
        assert_eq!(a, b);
        assert_eq!(a, 16);
    }

    #[test]
    fn finds_escape_in_second_half() {
        let mut s = *b"0123456789abcdefX";
        s[12] = b'\\';
        let i = find_quote_or_escape_fast(&s, 0, s.len());
        assert_eq!(i, 12);
    }
}
