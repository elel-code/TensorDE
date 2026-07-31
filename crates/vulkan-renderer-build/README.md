# vulkan-renderer-build

Cold-path Slang compiler for TensorDE's Vulkan rendering standard. Runtime
applications embed its validated SPIR-V output and do not link or distribute
Slang, LLVM, or SPIR-V Tools.

Foreign shader sources use a two-stage cold path: Slang's language frontend
first emits normalized HLSL-compatible Slang source, then
`lower_slang_bindings_to_descriptor_heap` replaces reflected register
resources with typed `DescriptorHandle<T>` accessors and a compact push-index
ABI. The normal compiler validates and optimizes that source into the only
artifact consumed by a runtime: native descriptor-heap SPIR-V.

The tool exposes explicit descriptor-free and native descriptor-heap
contracts. Descriptor resources must be lowered to typed `DescriptorHandle<T>`
accesses before compilation; mapped set/binding SPIR-V is rejected.

Generate and verify an asset with the pinned compiler:

```text
cargo run -p vulkan-renderer-build -- \
  compile source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  verify source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  lower-heap normalized.slang entryPoint native-heap.slang
```

`SLANGC` and `SPIRV_VAL` may name non-default tool paths. The compiler must be
exactly the version exported as `REQUIRED_SLANG_VERSION`; every output is also
validated for the Vulkan 1.4 target environment.

The `descriptor-heap` contract requests Slang's `spvDescriptorHeapEXT`
capability, requires `OpCapability DescriptorHeapEXT` and
`SPV_EXT_descriptor_heap`, and rejects every `Binding` or `DescriptorSet`
decoration. It also enables `-spirv-unified-descriptor-heap-stride`, so every
resource handle uses exactly `max(imageDescriptorSize, bufferDescriptorSize)`;
runtimes must pack resource heaps with that same stride, while sampler heaps
retain `samplerDescriptorSize`. The `descriptor-free` contract rejects those
decorations and the heap extension alike. Both contracts reject reflected
descriptor-table slots; the native heap contract accepts only push data plus
direct resource-heap access.
