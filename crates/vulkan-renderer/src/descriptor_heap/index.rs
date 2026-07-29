use crate::{Error, Result};

/// Converts a descriptor byte offset into the element index consumed by
/// `SPV_EXT_descriptor_heap` direct heap access.
///
/// `DescriptorHandle<T>` indexes a runtime array whose stride is the
/// driver-reported size of `T`'s descriptor. Byte offsets used by mapped
/// set/binding shaders therefore must not be passed to a native heap shader.
pub fn descriptor_heap_element_index(byte_offset: u64, descriptor_size: u64) -> Result<u32> {
    if descriptor_size == 0 {
        return Err(Error::Validation(
            "descriptor heap element size is zero".into(),
        ));
    }
    if !byte_offset.is_multiple_of(descriptor_size) {
        return Err(Error::Validation(format!(
            "descriptor heap byte offset {byte_offset} is not a multiple of descriptor size \
             {descriptor_size}"
        )));
    }
    u32::try_from(byte_offset / descriptor_size)
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
