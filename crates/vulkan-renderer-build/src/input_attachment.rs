//! Strict SPIR-V legalization for descriptor-heap input attachments.

use crate::{Error, Result};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const OP_CAPABILITY: u32 = 17;
const OP_TYPE_IMAGE: u32 = 25;
const OP_IMAGE_TEXEL_POINTER: u32 = 60;
const OP_IMAGE_SAMPLE_FIRST: u32 = 87;
const OP_IMAGE_READ: u32 = 98;
const OP_IMAGE_WRITE: u32 = 99;
const OP_ATOMIC_FIRST: u32 = 227;
const OP_ATOMIC_LAST: u32 = 242;
const CAPABILITY_INPUT_ATTACHMENT: u32 = 40;
const CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT: u32 = 56;
const DIM_2D: u32 = 1;
const DIM_SUBPASS_DATA: u32 = 6;

pub(crate) fn legalize_descriptor_heap_input_attachment(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut words = decode_words(bytes)?;
    let mut proxy_capabilities = 0;
    let mut image_types = 0;
    let mut image_reads = 0;
    visit_instructions_mut(&mut words, |opcode, operands| {
        match opcode {
            OP_CAPABILITY
                if operands.first() == Some(&CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT) =>
            {
                proxy_capabilities += 1;
                operands[0] = CAPABILITY_INPUT_ATTACHMENT;
            }
            OP_TYPE_IMAGE if is_exact_proxy_image(operands) => {
                image_types += 1;
                operands[2] = DIM_SUBPASS_DATA;
            }
            OP_IMAGE_READ => image_reads += 1,
            OP_IMAGE_TEXEL_POINTER | OP_IMAGE_SAMPLE_FIRST..=OP_IMAGE_WRITE
                if opcode != OP_IMAGE_READ =>
            {
                return Err(Error::SpirvContract(format!(
                    "input-attachment proxy contains forbidden image opcode {opcode}"
                )));
            }
            OP_ATOMIC_FIRST..=OP_ATOMIC_LAST => {
                return Err(Error::SpirvContract(format!(
                    "input-attachment proxy contains forbidden atomic opcode {opcode}"
                )));
            }
            _ => {}
        }
        Ok(())
    })?;
    require_exactly_one(proxy_capabilities, "storage-image proxy capability")?;
    require_exactly_one(image_types, "storage-image proxy type")?;
    require_exactly_one(image_reads, "exact-pixel image read")?;
    let bytes = encode_words(&words);
    validate_descriptor_heap_input_attachment(&bytes)?;
    Ok(bytes)
}

pub(crate) fn validate_descriptor_heap_input_attachment(bytes: &[u8]) -> Result<()> {
    let words = decode_words(bytes)?;
    let mut input_capabilities = 0;
    let mut proxy_capabilities = 0;
    let mut input_types = 0;
    let mut proxy_types = 0;
    let mut image_reads = 0;
    visit_instructions(&words, |opcode, operands| {
        match opcode {
            OP_CAPABILITY if operands.first() == Some(&CAPABILITY_INPUT_ATTACHMENT) => {
                input_capabilities += 1;
            }
            OP_CAPABILITY
                if operands.first() == Some(&CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT) =>
            {
                proxy_capabilities += 1;
            }
            OP_TYPE_IMAGE if is_exact_input_attachment(operands) => input_types += 1,
            OP_TYPE_IMAGE if is_exact_proxy_image(operands) => proxy_types += 1,
            OP_IMAGE_READ => image_reads += 1,
            OP_IMAGE_TEXEL_POINTER | OP_IMAGE_SAMPLE_FIRST..=OP_IMAGE_WRITE
                if opcode != OP_IMAGE_READ =>
            {
                return Err(Error::SpirvContract(format!(
                    "descriptor-heap input attachment contains forbidden image opcode {opcode}"
                )));
            }
            OP_ATOMIC_FIRST..=OP_ATOMIC_LAST => {
                return Err(Error::SpirvContract(format!(
                    "descriptor-heap input attachment contains forbidden atomic opcode {opcode}"
                )));
            }
            _ => {}
        }
        Ok(())
    })?;
    require_exactly_one(input_capabilities, "InputAttachment capability")?;
    require_exactly_one(input_types, "SubpassData image type")?;
    require_exactly_one(image_reads, "exact-pixel image read")?;
    if proxy_capabilities != 0 || proxy_types != 0 {
        return Err(Error::SpirvContract(
            "descriptor-heap input attachment retains its storage-image proxy".to_owned(),
        ));
    }
    Ok(())
}

fn is_exact_proxy_image(operands: &[u32]) -> bool {
    is_exact_image(operands, DIM_2D)
}

fn is_exact_input_attachment(operands: &[u32]) -> bool {
    is_exact_image(operands, DIM_SUBPASS_DATA)
}

fn is_exact_image(operands: &[u32], dimension: u32) -> bool {
    operands.len() == 8
        && operands[2] == dimension
        && operands[3] == 2
        && operands[4] == 0
        && operands[5] == 0
        && operands[6] == 2
        && operands[7] == 0
}

fn require_exactly_one(count: usize, label: &str) -> Result<()> {
    if count == 1 {
        Ok(())
    } else {
        Err(Error::SpirvContract(format!(
            "descriptor-heap input attachment requires exactly one {label}, found {count}"
        )))
    }
}

fn decode_words(bytes: &[u8]) -> Result<Vec<u32>> {
    if bytes.len() < 20 || !bytes.len().is_multiple_of(4) {
        return Err(Error::SpirvContract(format!(
            "generated SPIR-V has invalid byte length {}",
            bytes.len()
        )));
    }
    let words = bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte chunk")))
        .collect::<Vec<_>>();
    if words.first() != Some(&SPIRV_MAGIC) {
        return Err(Error::SpirvContract(
            "generated SPIR-V has an invalid magic word".to_owned(),
        ));
    }
    Ok(words)
}

fn encode_words(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn visit_instructions(
    words: &[u32],
    mut visit: impl FnMut(u32, &[u32]) -> Result<()>,
) -> Result<()> {
    let mut offset = 5;
    while offset < words.len() {
        let word_count = (words[offset] >> 16) as usize;
        if word_count == 0 || offset + word_count > words.len() {
            return Err(Error::SpirvContract(
                "SPIR-V contains a truncated instruction".to_owned(),
            ));
        }
        let opcode = words[offset] & 0xffff;
        visit(opcode, &words[offset + 1..offset + word_count])?;
        offset += word_count;
    }
    Ok(())
}

fn visit_instructions_mut(
    words: &mut [u32],
    mut visit: impl FnMut(u32, &mut [u32]) -> Result<()>,
) -> Result<()> {
    let mut offset = 5;
    while offset < words.len() {
        let word_count = (words[offset] >> 16) as usize;
        if word_count == 0 || offset + word_count > words.len() {
            return Err(Error::SpirvContract(
                "SPIR-V contains a truncated instruction".to_owned(),
            ));
        }
        let opcode = words[offset] & 0xffff;
        visit(opcode, &mut words[offset + 1..offset + word_count])?;
        offset += word_count;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legalizes_only_the_exact_storage_image_proxy_shape() {
        let proxy = module(&[
            instruction(
                OP_CAPABILITY,
                &[CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT],
            ),
            instruction(OP_TYPE_IMAGE, &[7, 6, DIM_2D, 2, 0, 0, 2, 0]),
            instruction(OP_IMAGE_READ, &[9, 10, 11, 12]),
        ]);
        let lowered = legalize_descriptor_heap_input_attachment(&proxy).unwrap();
        validate_descriptor_heap_input_attachment(&lowered).unwrap();
    }

    #[test]
    fn rejects_sampling_or_more_than_one_input_attachment() {
        let sampled = module(&[
            instruction(
                OP_CAPABILITY,
                &[CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT],
            ),
            instruction(OP_TYPE_IMAGE, &[7, 6, DIM_2D, 2, 0, 0, 2, 0]),
            instruction(OP_IMAGE_SAMPLE_FIRST, &[9, 10, 11, 12]),
        ]);
        assert!(legalize_descriptor_heap_input_attachment(&sampled).is_err());

        let duplicate = module(&[
            instruction(
                OP_CAPABILITY,
                &[CAPABILITY_STORAGE_IMAGE_WRITE_WITHOUT_FORMAT],
            ),
            instruction(OP_TYPE_IMAGE, &[7, 6, DIM_2D, 2, 0, 0, 2, 0]),
            instruction(OP_TYPE_IMAGE, &[8, 6, DIM_2D, 2, 0, 0, 2, 0]),
            instruction(OP_IMAGE_READ, &[9, 10, 11, 12]),
        ]);
        assert!(legalize_descriptor_heap_input_attachment(&duplicate).is_err());
    }

    fn module(instructions: &[Vec<u32>]) -> Vec<u8> {
        [
            vec![SPIRV_MAGIC, 0x0001_0600, 0, 16, 0],
            instructions.concat(),
        ]
        .concat()
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
    }

    fn instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
        let word_count = u32::try_from(operands.len() + 1).unwrap();
        std::iter::once((word_count << 16) | opcode)
            .chain(operands.iter().copied())
            .collect()
    }
}
