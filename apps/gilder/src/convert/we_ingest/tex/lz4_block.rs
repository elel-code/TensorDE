//! Minimal LZ4 block decoder for WE TEXB level payloads.

use super::TexParseError;

pub(super) fn decode_lz4_block(
    payload: &[u8],
    decoded_size: usize,
) -> Result<Vec<u8>, TexParseError> {
    let mut output = Vec::with_capacity(decoded_size);
    let mut offset = 0usize;
    while offset < payload.len() {
        let token = payload[offset];
        offset += 1;
        let literal_len = extended_length(payload, &mut offset, (token >> 4) as usize)?;
        let literal_end = offset
            .checked_add(literal_len)
            .ok_or(TexParseError::OffsetOverflow)?;
        let literals = payload
            .get(offset..literal_end)
            .ok_or_else(|| TexParseError::Lz4("truncated literal run".to_owned()))?;
        output.extend_from_slice(literals);
        offset = literal_end;
        if offset == payload.len() {
            break;
        }
        let match_bytes = payload
            .get(offset..offset.saturating_add(2))
            .ok_or_else(|| TexParseError::Lz4("truncated match offset".to_owned()))?;
        let match_offset = u16::from_le_bytes(match_bytes.try_into().expect("u16 slice")) as usize;
        offset += 2;
        if match_offset == 0 || match_offset > output.len() {
            return Err(TexParseError::Lz4(format!(
                "invalid match offset {match_offset} at output byte {}",
                output.len()
            )));
        }
        let match_len = extended_length(payload, &mut offset, (token & 0x0f) as usize)? + 4;
        for _ in 0..match_len {
            let source = output.len() - match_offset;
            let byte = output[source];
            output.push(byte);
            if output.len() > decoded_size {
                return Err(TexParseError::Lz4(format!(
                    "decoded more than declared {decoded_size} bytes"
                )));
            }
        }
    }
    if output.len() != decoded_size {
        return Err(TexParseError::Lz4(format!(
            "decoded {} bytes, expected {decoded_size}",
            output.len()
        )));
    }
    Ok(output)
}

fn extended_length(
    payload: &[u8],
    offset: &mut usize,
    base: usize,
) -> Result<usize, TexParseError> {
    if base != 15 {
        return Ok(base);
    }
    let mut length = base;
    loop {
        let extra = *payload
            .get(*offset)
            .ok_or_else(|| TexParseError::Lz4("truncated extended length".to_owned()))?
            as usize;
        *offset += 1;
        length = length
            .checked_add(extra)
            .ok_or(TexParseError::OffsetOverflow)?;
        if extra != 255 {
            return Ok(length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_overlapping_matches() {
        let payload = [0x32, b'a', b'b', b'c', 3, 0];
        assert_eq!(decode_lz4_block(&payload, 9).expect("lz4"), b"abcabcabc");
    }
}
