use super::*;

#[test]
fn typed_load_and_store_ops_map_without_implicit_preservation() {
    assert_eq!(
        load_op(LoadOp::Clear(0.5), |depth| vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 },
        })
        .0,
        vk::AttachmentLoadOp::CLEAR
    );
    assert_eq!(
        match StoreOp::Discard {
            StoreOp::Store => vk::AttachmentStoreOp::STORE,
            StoreOp::Discard => vk::AttachmentStoreOp::DONT_CARE,
        },
        vk::AttachmentStoreOp::DONT_CARE
    );
}
