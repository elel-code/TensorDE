//! SWAR helpers inspired by Glaze `util/parse.hpp`.

/// Broadcast a byte across all 8 lanes of a `u64`.
#[inline(always)]
pub const fn repeat_byte(byte: u8) -> u64 {
    0x0101_0101_0101_0101_u64.wrapping_mul(byte as u64)
}

/// Load up to 8 bytes little-endian; missing high bytes are zero.
#[inline(always)]
pub fn load_u64_unaligned(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    let n = bytes.len().min(8);
    buf[..n].copy_from_slice(&bytes[..n]);
    u64::from_le_bytes(buf)
}

/// True if any lane equals `byte`.
#[inline(always)]
pub const fn has_byte(chunk: u64, byte: u8) -> u64 {
    let x = chunk ^ repeat_byte(byte);
    // For each lane: zero iff equal. Classic SWAR zero-byte detect.
    (x.wrapping_sub(repeat_byte(0x01))) & !x & repeat_byte(0x80)
}

/// Lanes that are ASCII space (0x20) or tab (0x09) — common KDL horizontal ws.
#[inline(always)]
pub const fn has_ascii_horizontal_ws(chunk: u64) -> u64 {
    has_byte(chunk, b' ') | has_byte(chunk, b'\t')
}

/// First matching lane index (0..8), or 8 if none. Requires `mask != 0`.
#[inline(always)]
pub fn first_lane(mask: u64) -> usize {
    (mask.trailing_zeros() as usize) >> 3
}

/// Skip runs of ASCII space/tab using 8-byte steps.
///
/// Returns the new index. Does not handle unicode spaces or comments.
#[inline(always)]
pub fn skip_ascii_horizontal_ws(input: &[u8], mut index: usize) -> usize {
    let len = input.len();
    while index + 8 <= len {
        let chunk = load_u64_unaligned(&input[index..index + 8]);
        let ws = has_ascii_horizontal_ws(chunk);
        // All eight lanes are space/tab if every high bit of the equal-mask is set
        // for positions that are ws — simpler: check byte-by-byte in chunk when mixed.
        let mut all_ws = true;
        let mut advance = 0usize;
        for i in 0..8 {
            let b = input[index + i];
            if b == b' ' || b == b'\t' {
                advance += 1;
            } else {
                all_ws = false;
                break;
            }
        }
        if all_ws {
            index += 8;
            let _ = ws;
            continue;
        }
        index += advance;
        break;
    }
    while index < len && (input[index] == b' ' || input[index] == b'\t') {
        index += 1;
    }
    index
}

/// Scan forward for `"` or `\` in a quoted string body (single-line, no validation).
/// Returns absolute byte index of the first special, or `end` if none in range.
#[inline(always)]
pub fn find_quote_or_escape(input: &[u8], mut index: usize, end: usize) -> usize {
    while index + 8 <= end {
        let chunk = load_u64_unaligned(&input[index..index + 8]);
        let hit = has_byte(chunk, b'"') | has_byte(chunk, b'\\');
        if hit != 0 {
            return index + first_lane(hit);
        }
        // Also stop on ASCII control (< 0x20) roughly: any byte with high bits clear in low 5?
        // For speed we only SWAR quote/escape; controls checked on the slow path.
        index += 8;
    }
    while index < end {
        let b = input[index];
        if b == b'"' || b == b'\\' {
            return index;
        }
        index += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_spaces() {
        let s = b"        foo";
        assert_eq!(skip_ascii_horizontal_ws(s, 0), 8);
        let s2 = b"  \t bar";
        assert_eq!(skip_ascii_horizontal_ws(s2, 0), 4);
    }

    #[test]
    fn find_quote() {
        let s = b"hello\"world";
        assert_eq!(find_quote_or_escape(s, 0, s.len()), 5);
        let long = b"01234567quote\"";
        assert_eq!(find_quote_or_escape(long, 0, long.len()), 13);
    }
}
