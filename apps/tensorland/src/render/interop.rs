//! Tensor's native KMS policy consumes the shared renderer's value-only
//! Linux dma-buf capability gate. Vulkan probing and sync-file validation no
//! longer live in the compositor product.

pub use vulkan_renderer::LinuxDmaBufCapabilities as NativeInteropCapabilities;
