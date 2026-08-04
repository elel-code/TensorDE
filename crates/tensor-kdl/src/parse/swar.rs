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

/// Skip ASCII space/tab up to logical `end` (may SWAR-load into padding past `end`).
///
/// When `input.len() == end` this is the classic unpadded path. When `input` is a
/// [`crate::PaddedInput`] buffer, pass `end = content_len` so EOF is correct while
/// u64 loads may touch zero padding (Glaze `padding_bytes`).
///
/// Cite: Glaze `util/parse.hpp` + `opts.hpp` `padding_bytes`.
#[inline(always)]
pub fn skip_ascii_horizontal_ws(input: &[u8], mut index: usize, end: usize) -> usize {
    let end = end.min(input.len());
    // All eight high-bits set ⇒ every lane matched space or tab.
    const ALL_WS: u64 = 0x8080_8080_8080_8080;
    while index < end {
        // Prefer full u64 when padding or content supplies 8 bytes.
        if index + 8 <= input.len() {
            let chunk = load_u64_unaligned(&input[index..index + 8]);
            let ws = has_ascii_horizontal_ws(chunk);
            if index + 8 <= end && ws == ALL_WS {
                index += 8;
                continue;
            }
            let non_ws = ALL_WS & !ws;
            if non_ws != 0 {
                let lane = first_lane(non_ws);
                let at = index + lane;
                return at.min(end);
            }
            // All ws in this u64 but straddles logical end — advance to end.
            if index + 8 > end {
                return end;
            }
            index += 8;
            continue;
        }
        break;
    }
    while index < end && (input[index] == b' ' || input[index] == b'\t') {
        index += 1;
    }
    index
}

/// Scan forward for `"` or `\` in a quoted string body (single-line, no validation).
/// Returns absolute byte index of the first special, or `end` if none in range.
#[inline(always)]
pub fn find_quote_or_escape(input: &[u8], mut index: usize, end: usize) -> usize {
    let end = end.min(input.len());
    while index < end {
        if index + 8 <= input.len() {
            let chunk = load_u64_unaligned(&input[index..index + 8]);
            let hit = has_byte(chunk, b'"') | has_byte(chunk, b'\\');
            if hit != 0 {
                let at = index + first_lane(hit);
                return at.min(end);
            }
            if index + 8 > end {
                // Remainder inside this chunk past logical end is padding — stop.
                break;
            }
            index += 8;
            continue;
        }
        break;
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

/// Scan forward for a quote byte only.
///
/// Raw KDL delimiters do not interpret escapes, so stopping at `\\` would add
/// needless work. This keeps their delimiter scan on the same portable SWAR
/// path as Glaze's direct string scans (`util/parse.hpp`).
#[inline(always)]
pub fn find_quote(input: &[u8], mut index: usize, end: usize) -> usize {
    let end = end.min(input.len());
    while index < end {
        if index + 8 <= input.len() {
            let chunk = load_u64_unaligned(&input[index..index + 8]);
            let hit = has_byte(chunk, b'"');
            if hit != 0 {
                let at = index + first_lane(hit);
                return at.min(end);
            }
            if index + 8 > end {
                // Remainder inside this chunk past logical end is padding — stop.
                break;
            }
            index += 8;
            continue;
        }
        break;
    }
    while index < end {
        if input[index] == b'"' {
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
        assert_eq!(skip_ascii_horizontal_ws(s, 0, s.len()), 8);
        let s2 = b"  \t bar";
        assert_eq!(skip_ascii_horizontal_ws(s2, 0, s2.len()), 4);
        let s3 = b"   x    rest";
        assert_eq!(skip_ascii_horizontal_ws(s3, 0, s3.len()), 3);
        let s4 = b"                end";
        assert_eq!(skip_ascii_horizontal_ws(s4, 0, s4.len()), 16);
        // Padded: logical end mid-buffer.
        let mut pad = b"  x\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        pad[0] = b' ';
        pad[1] = b' ';
        pad[2] = b'x';
        assert_eq!(skip_ascii_horizontal_ws(&pad, 0, 3), 2);
    }

    #[test]
    fn find_quote() {
        let s = b"hello\"world";
        assert_eq!(find_quote_or_escape(s, 0, s.len()), 5);
        let long = b"01234567quote\"";
        assert_eq!(find_quote_or_escape(long, 0, long.len()), 13);
    }

    #[test]
    fn find_quote_ignores_raw_backslashes() {
        let s = b"raw\\content\"close";
        assert_eq!(super::find_quote(s, 0, s.len()), 11);
        assert_eq!(super::find_quote(b"no quote", 0, 8), 8);
    }
}
