# Architecture

Tensor is a Rust Wayland compositor built around four ownership domains:

1. Smithay owns protocol objects, input/session state, and the calloop event loop.
2. Bevy ECS owns compositor intent: stable IDs, lifecycle, workspace membership, focus, and geometry.
3. Vulkanalia owns GPU handles, descriptor heaps, frame extraction, synchronization, and KMS output.
4. IPC and portal adapters translate external requests into validated ECS commands.

Smithay and Vulkan objects do not become ordinary ECS components. Thread-affine Smithay state stays
in the protocol owner or a Bevy `NonSend` resource. The renderer consumes a compact scene extracted
from ECS once per frame rather than issuing ECS queries in GPU submission loops.

The renderer requires Vulkan 1.4 plus `VK_EXT_descriptor_heap`. Descriptor sets and descriptor
buffers are not alternative backends. A device that lacks the heap capability fails startup before
any long-lived renderer state is created.

Physical-device enumeration and ranking live in `render/device.rs`. The policy is configurable but
the default prefers a discrete GPU, then integrated/virtual hardware, with CPU devices last. This
policy is separate from Vulkan instance/device creation so probing can be tested without a GPU.

Modules use `foo.rs` plus `foo/*.rs`; `mod.rs` is prohibited. Shared dependency-light primitives
belong in `crates/tensor-util`, while protocol, renderer, and compositor-specific types stay in their
own crates/modules.
