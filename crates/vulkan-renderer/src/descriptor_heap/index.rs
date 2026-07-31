use crate::{Error, Result};

/// Converts a descriptor byte offset into the element index consumed by
/// `SPV_EXT_descriptor_heap` direct heap access.
///
/// `DescriptorHandle<T>` indexes a runtime array whose stride is fixed by the
/// shader compiler contract. TensorDE compiles resource heaps with Slang's
/// unified descriptor-heap stride, while sampler heaps retain the sampler
/// descriptor stride. Byte offsets must therefore be divided by the matching
/// heap ABI stride, never by a descriptor kind inferred at the call site.
pub fn descriptor_heap_element_index(byte_offset: u64, element_stride: u64) -> Result<u32> {
    if element_stride == 0 {
        return Err(Error::Validation(
            "descriptor heap element stride is zero".into(),
        ));
    }
    if !byte_offset.is_multiple_of(element_stride) {
        return Err(Error::Validation(format!(
            "descriptor heap byte offset {byte_offset} is not a multiple of element stride \
             {element_stride}"
        )));
    }
    u32::try_from(byte_offset / element_stride)
        .map_err(|_| Error::Validation("descriptor heap element index exceeds u32".into()))
}

#[cfg(test)]
mod tests {
    use super::descriptor_heap_element_index;

    #[test]
    fn native_heap_index_uses_descriptor_elements_not_bytes() {
        assert_eq!(descriptor_heap_element_index(96, 32).unwrap(), 3);
        assert!(descriptor_heap_element_index(80, 32).is_err());
        assert!(descriptor_heap_element_index(0, 0).is_err());
    }
}
