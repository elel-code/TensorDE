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

The tool exposes explicit descriptor-free, native descriptor-heap, and mapped
descriptor-heap contracts. Both heap contracts require a
`VK_EXT_descriptor_heap` runtime; neither permits descriptor-set allocation or
binding.

Generate and verify an asset with the pinned compiler:

```text
cargo run -p vulkan-renderer-build -- \
  compile source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  verify source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  compile mapped.slang entryPoint fragment output.spv 0 mapped-descriptor-heap
cargo run -p vulkan-renderer-build -- \
  lower-heap normalized.slang entryPoint native-heap.slang
```

`SLANGC` and `SPIRV_VAL` may name non-default tool paths. The compiler must be
exactly the version exported as `REQUIRED_SLANG_VERSION`; every output is also
validated for the Vulkan 1.4 target environment.

The `descriptor-heap` contract requests Slang's `spvDescriptorHeapEXT`
capability, requires `OpCapability DescriptorHeapEXT` and
`SPV_EXT_descriptor_heap`, and rejects every `Binding` or `DescriptorSet`
decoration. The `descriptor-free` contract rejects those decorations and the
heap extension alike. The `mapped-descriptor-heap` contract accepts only
paired `Binding` and `DescriptorSet` decorations and rejects native
`DescriptorHeapEXT` instructions; pipelines must map every declaration to a
heap range through `VkShaderDescriptorSetAndBindingMappingInfoEXT`.
