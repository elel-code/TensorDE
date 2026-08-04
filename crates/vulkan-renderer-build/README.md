# vulkan-renderer-build

Cold-path Slang compiler for TensorDE's Vulkan rendering standard. Runtime
applications embed its validated SPIR-V output and do not link or distribute
Slang, LLVM, or SPIR-V Tools.

Slang sources use a two-stage cold path:
`lower_slang_bindings_to_descriptor_heap` replaces direct register resources
with typed `DescriptorHandle<T>` accessors and a compact push-index ABI. The
normal compiler validates and optimizes that source into the only artifact
consumed by a runtime: descriptor-heap SPIR-V. This crate deliberately
does not provide a GLSL or HLSL frontend route.

The tool exposes explicit descriptor-free and descriptor-heap
contracts. Descriptor resources must be lowered to typed `DescriptorHandle<T>`
accesses before compilation; mapped set/binding SPIR-V is rejected.

Generate and verify an asset with the pinned compiler:

```text
cargo run -p vulkan-renderer-build -- \
  compile source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  verify source.slang entryPoint fragment output.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  lower-heap normalized.slang entryPoint descriptor-heap.slang
```

`SLANGC` and `SPIRV_VAL` may name non-default tool paths. The compiler must be
exactly the version exported as `REQUIRED_SLANG_VERSION`; every output is also
validated for the Vulkan 1.4 target environment.

TensorDE's canonical local compiler is the complete official distribution in
the ignored `artifacts/tools/slang/2026.14.1` directory. Select it from the
workspace root with:

```sh
export SLANGC="$PWD/artifacts/tools/slang/2026.14.1/bin/slangc"
```

Keep the distribution's `bin`, `lib`, `include`, licenses, and support files
together. Only the generated, validated SPIR-V is a runtime or distribution
input.

The `descriptor-heap` contract requests Slang's `spvDescriptorHeapEXT`
capability, requires `OpCapability DescriptorHeapEXT` and
`SPV_EXT_descriptor_heap`, and rejects every `Binding` or `DescriptorSet`
decoration. It also enables `-spirv-unified-descriptor-heap-stride`, so every
resource handle uses one image/buffer stride; runtimes must pack resource heaps
with that same maximum stride. Sampler heaps keep their independent stride.
The `descriptor-free` contract rejects those decorations and the heap extension
alike. Both contracts reject reflected descriptor-table slots; the descriptor heap
contract accepts only push data plus direct resource-heap access.

The pinned Slang release must preserve source-level buffer kinds. A mixed
instruction regression compiles both direct-heap constant-buffer forms and
requires exactly one Uniform heap pointer while `StructuredBuffer<T>` and
`RWStructuredBuffer<T>` remain StorageBuffer pointers. This guards the
descriptor-heap lowering fix shipped in Slang 2026.14.1.
