use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

use crate::{
    Error, REQUIRED_SLANG_VERSION, Result, SPIRV_PROFILE, ShaderContract, ShaderStage,
    VULKAN_TARGET_ENVIRONMENT, input_attachment,
};

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
const SLANG_OPTIMIZATION_LEVEL: &str = "-O2";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderCompileRequest {
    pub source: PathBuf,
    pub entry_point: String,
    pub stage: ShaderStage,
    pub output: PathBuf,
    pub contract: ShaderContract,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileReport {
    pub output: PathBuf,
    pub spirv_bytes: usize,
    pub reflection: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlangCompiler {
    pub(crate) slangc: PathBuf,
    spirv_val: PathBuf,
}

impl Default for SlangCompiler {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl SlangCompiler {
    pub fn from_environment() -> Self {
        Self {
            slangc: environment_tool("SLANGC", "slangc"),
            spirv_val: environment_tool("SPIRV_VAL", "spirv-val"),
        }
    }

    pub fn compile(&self, request: &ShaderCompileRequest) -> Result<CompileReport> {
        self.compile_with_legalization(request, false)
    }

    /// Reflects a directly-authored native Slang stage before descriptor-heap
    /// lowering. The temporary SPIR-V is validated and discarded; it exists
    /// only to preserve typed uniform layout metadata for a later native-heap
    /// production compile.
    pub fn reflect_native_source(
        &self,
        source: &Path,
        entry_point: &str,
        stage: ShaderStage,
        output: &Path,
    ) -> Result<Value> {
        self.check_tools()?;
        let temporary = TemporaryOutputs::new(output);
        self.run_native_slang(source, entry_point, stage, &temporary, false, true)?;
        self.validate_spirv(&temporary.spirv)?;
        read_reflection(&temporary.reflection)
    }

    /// Compiles a native resource-heap storage-image proxy and legalizes it to
    /// a strictly validated Vulkan input attachment.
    pub fn compile_input_attachment(
        &self,
        request: &ShaderCompileRequest,
    ) -> Result<CompileReport> {
        if request.stage != ShaderStage::Fragment
            || !request.contract.emits_native_descriptor_heap()
        {
            return Err(Error::SpirvContract(
                "native input attachments require a fragment-stage descriptor-heap contract"
                    .to_owned(),
            ));
        }
        self.compile_with_legalization(request, true)
    }

    fn compile_with_legalization(
        &self,
        request: &ShaderCompileRequest,
        input_attachment: bool,
    ) -> Result<CompileReport> {
        self.check_tools()?;
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| Error::io("create shader output directory", parent, error))?;
        }
        let temporary = TemporaryOutputs::new(&request.output);
        self.run_slang(request, &temporary)?;
        let reflection = read_reflection(&temporary.reflection)?;
        request
            .contract
            .validate(&reflection, &request.entry_point, request.stage)?;
        if input_attachment {
            let proxy = fs::read(&temporary.spirv).map_err(|error| {
                Error::io(
                    "read input-attachment proxy SPIR-V",
                    &temporary.spirv,
                    error,
                )
            })?;
            let lowered = input_attachment::legalize_native_input_attachment(&proxy)?;
            fs::write(&temporary.spirv, lowered).map_err(|error| {
                Error::io(
                    "write legalized input-attachment SPIR-V",
                    &temporary.spirv,
                    error,
                )
            })?;
        }
        self.validate_spirv(&temporary.spirv)?;
        let bytes = fs::read(&temporary.spirv)
            .map_err(|error| Error::io("read generated SPIR-V", &temporary.spirv, error))?;
        validate_word_length(&bytes)?;
        validate_descriptor_contract(&bytes, request.contract)?;
        if input_attachment {
            input_attachment::validate_native_input_attachment(&bytes)?;
        }
        fs::rename(&temporary.spirv, &request.output)
            .map_err(|error| Error::io("install generated SPIR-V", &request.output, error))?;
        Ok(CompileReport {
            output: request.output.clone(),
            spirv_bytes: bytes.len(),
            reflection,
        })
    }

    pub fn verify(&self, request: &ShaderCompileRequest) -> Result<CompileReport> {
        let expected = fs::read(&request.output)
            .map_err(|error| Error::io("read checked-in SPIR-V", &request.output, error))?;
        let temporary_output = temporary_path(&request.output, "verification.spv");
        let temporary_request = ShaderCompileRequest {
            output: temporary_output.clone(),
            ..request.clone()
        };
        let report = self.compile(&temporary_request)?;
        let generated = fs::read(&temporary_output)
            .map_err(|error| Error::io("read verification SPIR-V", &temporary_output, error))?;
        let _ = fs::remove_file(&temporary_output);
        if generated != expected {
            return Err(Error::ArtifactMismatch {
                path: request.output.clone(),
                expected_bytes: expected.len(),
                generated_bytes: generated.len(),
            });
        }
        Ok(CompileReport {
            output: request.output.clone(),
            spirv_bytes: report.spirv_bytes,
            reflection: report.reflection,
        })
    }

    pub(crate) fn check_slang(&self) -> Result<()> {
        let version = run(&self.slangc, [OsString::from("-version")])?;
        let version_bytes = if version.stdout.is_empty() {
            &version.stderr
        } else {
            &version.stdout
        };
        let found = String::from_utf8_lossy(version_bytes).trim().to_owned();
        if found != REQUIRED_SLANG_VERSION {
            return Err(Error::CompilerVersion {
                expected: REQUIRED_SLANG_VERSION,
                found,
            });
        }
        Ok(())
    }

    fn check_tools(&self) -> Result<()> {
        self.check_slang()?;
        run(&self.spirv_val, [OsString::from("--version")])?;
        Ok(())
    }

    fn run_slang(
        &self,
        request: &ShaderCompileRequest,
        temporary: &TemporaryOutputs,
    ) -> Result<()> {
        self.run_native_slang(
            &request.source,
            &request.entry_point,
            request.stage,
            temporary,
            request.contract.emits_native_descriptor_heap(),
            false,
        )
    }

    fn run_native_slang(
        &self,
        source: &Path,
        entry_point: &str,
        stage: ShaderStage,
        temporary: &TemporaryOutputs,
        descriptor_heap: bool,
        direct_reflection_bindings: bool,
    ) -> Result<()> {
        let mut arguments = vec![
            source.as_os_str().to_owned(),
            "-entry".into(),
            entry_point.into(),
            "-stage".into(),
            stage.slang_name().into(),
            "-target".into(),
            "spirv".into(),
            "-profile".into(),
            SPIRV_PROFILE.into(),
            "-std".into(),
            "2026".into(),
            "-matrix-layout-row-major".into(),
            SLANG_OPTIMIZATION_LEVEL.into(),
            "-warnings-as-errors".into(),
            "all".into(),
            "-restrictive-capability-check".into(),
            "-emit-spirv-directly".into(),
            "-reflection-json".into(),
            temporary.reflection.as_os_str().to_owned(),
            "-o".into(),
            temporary.spirv.as_os_str().to_owned(),
        ];
        if descriptor_heap {
            let output_position = arguments.len() - 2;
            arguments.splice(
                output_position..output_position,
                native_descriptor_heap_arguments(),
            );
        } else if direct_reflection_bindings {
            let output_position = arguments.len() - 2;
            arguments.splice(
                output_position..output_position,
                native_direct_reflection_binding_arguments(),
            );
        }
        run(&self.slangc, arguments).map(|_| ())
    }

    fn validate_spirv(&self, path: &Path) -> Result<()> {
        run(
            &self.spirv_val,
            [
                OsString::from("--target-env"),
                OsString::from(VULKAN_TARGET_ENVIRONMENT),
                path.as_os_str().to_owned(),
            ],
        )
        .map(|_| ())
    }
}

fn native_descriptor_heap_arguments() -> [OsString; 3] {
    [
        OsString::from("-capability"),
        OsString::from("spvDescriptorHeapEXT"),
        OsString::from("-spirv-unified-descriptor-heap-stride"),
    ]
}

/// Direct native reflection retains authored registers solely to recover typed
/// uniform layout on the cold path. Vulkan requires those registers to map to
/// distinct bindings even though the production source immediately replaces
/// them with descriptor-heap handles.
fn native_direct_reflection_binding_arguments() -> [OsString; 12] {
    [
        OsString::from("-fvk-b-shift"),
        OsString::from("0"),
        OsString::from("0"),
        OsString::from("-fvk-t-shift"),
        OsString::from("1024"),
        OsString::from("0"),
        OsString::from("-fvk-s-shift"),
        OsString::from("2048"),
        OsString::from("0"),
        OsString::from("-fvk-u-shift"),
        OsString::from("3072"),
        OsString::from("0"),
    ]
}

fn environment_tool(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

pub(crate) fn run(tool: &Path, arguments: impl IntoIterator<Item = OsString>) -> Result<Output> {
    let output = Command::new(tool)
        .args(arguments)
        .output()
        .map_err(|source| Error::ToolLaunch {
            tool: tool.to_owned(),
            source,
        })?;
    if output.status.success() {
        return Ok(output);
    }
    Err(Error::ToolFailure {
        tool: tool.to_owned(),
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn read_reflection(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|error| Error::io("read Slang reflection", path, error))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| Error::Reflection(format!("invalid JSON from Slang: {error}")))
}

fn validate_word_length(bytes: &[u8]) -> Result<()> {
    if bytes.len() >= 20 && bytes.len().is_multiple_of(4) {
        Ok(())
    } else {
        Err(Error::SpirvContract(format!(
            "generated SPIR-V has invalid byte length {}",
            bytes.len()
        )))
    }
}

fn validate_descriptor_contract(bytes: &[u8], contract: ShaderContract) -> Result<()> {
    let words = bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte chunk")))
        .collect::<Vec<_>>();
    let mut descriptor_heap_capability = false;
    let mut descriptor_heap_extension = false;
    let mut binding_ids = BTreeSet::new();
    let mut descriptor_set_ids = BTreeSet::new();
    let mut offset = 5;
    while offset < words.len() {
        let instruction_words = (words[offset] >> 16) as usize;
        let opcode = words[offset] & 0xffff;
        let operands = &words[offset + 1..offset + instruction_words];
        match opcode {
            10 => {
                descriptor_heap_extension |=
                    decode_spirv_string(operands) == "SPV_EXT_descriptor_heap";
            }
            17 => descriptor_heap_capability |= operands.first() == Some(&5_128),
            71 => {
                if let (Some(id), Some(decoration)) = (operands.first(), operands.get(1)) {
                    match decoration {
                        33 => {
                            binding_ids.insert(*id);
                        }
                        34 => {
                            descriptor_set_ids.insert(*id);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        offset += instruction_words;
    }

    if !binding_ids.is_empty() || !descriptor_set_ids.is_empty() {
        return Err(Error::SpirvContract(
            "SPIR-V contains forbidden Binding or DescriptorSet decorations".to_owned(),
        ));
    }
    if contract.emits_native_descriptor_heap()
        && !(descriptor_heap_capability && descriptor_heap_extension)
    {
        return Err(Error::SpirvContract(
            "descriptor-heap shader lacks DescriptorHeapEXT capability or SPV_EXT_descriptor_heap"
                .to_owned(),
        ));
    }
    if !contract.emits_native_descriptor_heap()
        && (descriptor_heap_capability || descriptor_heap_extension)
    {
        return Err(Error::SpirvContract(
            "descriptor-free shader unexpectedly uses SPV_EXT_descriptor_heap".to_owned(),
        ));
    }
    Ok(())
}

fn decode_spirv_string(words: &[u32]) -> String {
    let bytes = words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

struct TemporaryOutputs {
    spirv: PathBuf,
    reflection: PathBuf,
}

impl TemporaryOutputs {
    fn new(output: &Path) -> Self {
        Self {
            spirv: temporary_path(output, "spv"),
            reflection: temporary_path(output, "reflection.json"),
        }
    }
}

impl Drop for TemporaryOutputs {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.spirv);
        let _ = fs::remove_file(&self.reflection);
    }
}

fn temporary_path(output: &Path, suffix: &str) -> PathBuf {
    let id = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shader");
    output.with_file_name(format!(
        ".{name}.{}.{}.tmp.{suffix}",
        std::process::id(),
        id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORAGE_CLASS_UNIFORM: u32 = 2;
    const STORAGE_CLASS_STORAGE_BUFFER: u32 = 12;
    const OP_TYPE_POINTER: u32 = 32;
    const OP_TYPE_UNTYPED_POINTER_KHR: u32 = 4_417;
    const OP_BUFFER_POINTER_EXT: u32 = 5_119;

    #[test]
    fn production_slang_uses_high_optimization() {
        assert_eq!(SLANG_OPTIMIZATION_LEVEL, "-O2");
    }

    #[test]
    fn native_descriptor_heap_uses_one_resource_array_stride() {
        assert_eq!(
            native_descriptor_heap_arguments(),
            [
                OsString::from("-capability"),
                OsString::from("spvDescriptorHeapEXT"),
                OsString::from("-spirv-unified-descriptor-heap-stride"),
            ]
        );
    }

    #[test]
    fn direct_native_reflection_partitions_legacy_register_classes() {
        assert_eq!(
            native_direct_reflection_binding_arguments(),
            [
                OsString::from("-fvk-b-shift"),
                OsString::from("0"),
                OsString::from("0"),
                OsString::from("-fvk-t-shift"),
                OsString::from("1024"),
                OsString::from("0"),
                OsString::from("-fvk-s-shift"),
                OsString::from("2048"),
                OsString::from("0"),
                OsString::from("-fvk-u-shift"),
                OsString::from("3072"),
                OsString::from("0"),
            ]
        );
    }

    #[test]
    fn direct_native_reflection_compiles_authored_registers_without_a_frontend() {
        let base = std::env::temp_dir().join(format!(
            "vulkan-renderer-build-direct-reflection-{}",
            std::process::id()
        ));
        let source_path = base.with_extension("slang");
        let heap_source_path = base.with_extension("heap.slang");
        let output_path = base.with_extension("spv");
        let direct = crate::lower_generated_stage_to_native_slang(
            r#"layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
uniform mat4 g_ModelViewProjectionMatrix;
uniform float g_Time;
uniform sampler2D g_Texture0;
void main()
{
    vec4 transformed = mul(vec4(v_TexCoord, 0.0, 1.0), g_ModelViewProjectionMatrix);
    o_Color = texture2D(g_Texture0, v_TexCoord) + vec4(transformed.x + g_Time);
}"#,
            ShaderStage::Fragment,
        )
        .unwrap();
        fs::write(&source_path, &direct).unwrap();

        let compiler = SlangCompiler::from_environment();
        let reflection = compiler
            .reflect_native_source(&source_path, "main", ShaderStage::Fragment, &output_path)
            .unwrap();

        assert!(
            reflection["parameters"]
                .as_array()
                .is_some_and(|parameters| {
                    parameters.iter().any(|parameter| {
                        parameter["name"] == "GilderUniforms0"
                            && parameter["binding"]["kind"] == "constantBuffer"
                            && parameter["binding"]["index"] == 0
                    })
                })
        );
        let heap = crate::lower_slang_bindings_to_descriptor_heap(&direct, "main").unwrap();
        fs::write(&heap_source_path, &heap.source).unwrap();
        compiler
            .compile(&ShaderCompileRequest {
                source: heap_source_path.clone(),
                entry_point: "main".to_owned(),
                stage: ShaderStage::Fragment,
                output: output_path.clone(),
                contract: ShaderContract::descriptor_heap(u64::from(heap.push_constant_bytes)),
            })
            .unwrap();
        let _ = fs::remove_file(source_path);
        let _ = fs::remove_file(heap_source_path);
        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn slang_keeps_direct_heap_constant_and_structured_buffers_distinct() {
        for (case, constant_expression) in [
            (
                "descriptor-handle",
                "DescriptorHandle<ConstantBuffer<UniformData>>(pushData.uniformIndex)",
            ),
            (
                "resource-heap",
                "ResourceDescriptorHeap[pushData.uniformIndex]",
            ),
        ] {
            let source = format!(
                r#"
struct UniformData {{ float4 tint; }};
struct StorageData {{ float4 value; }};
struct PushData {{ uint uniformIndex; uint inputIndex; uint outputIndex; }};
[[vk::push_constant]] ConstantBuffer<PushData> pushData;
[[shader("compute")]]
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{{
    ConstantBuffer<UniformData> uniformData = {constant_expression};
    StructuredBuffer<StorageData> inputData =
        DescriptorHandle<StructuredBuffer<StorageData>>(pushData.inputIndex);
    RWStructuredBuffer<StorageData> outputData =
        DescriptorHandle<RWStructuredBuffer<StorageData>>(pushData.outputIndex);
    outputData[id.x].value = inputData[id.x].value * uniformData.tint;
}}
"#
            );
            let base = std::env::temp_dir().join(format!(
                "vulkan-renderer-build-mixed-buffer-{}-{case}",
                std::process::id()
            ));
            let source_path = base.with_extension("slang");
            let output_path = base.with_extension("spv");
            fs::write(&source_path, source).unwrap();
            SlangCompiler::from_environment()
                .compile(&ShaderCompileRequest {
                    source: source_path.clone(),
                    entry_point: "main".to_owned(),
                    stage: ShaderStage::Compute,
                    output: output_path.clone(),
                    contract: ShaderContract::descriptor_heap(12),
                })
                .unwrap();

            let bytes = fs::read(&output_path).unwrap();
            let classes = buffer_pointer_storage_classes(&bytes);
            assert_eq!(
                classes
                    .iter()
                    .filter(|class| **class == STORAGE_CLASS_UNIFORM)
                    .count(),
                1,
                "{case} constant buffer did not use exactly one Uniform heap pointer"
            );
            assert_eq!(
                classes
                    .iter()
                    .filter(|class| **class == STORAGE_CLASS_STORAGE_BUFFER)
                    .count(),
                2,
                "{case} structured buffers did not retain two StorageBuffer heap pointers"
            );
            let _ = fs::remove_file(source_path);
            let _ = fs::remove_file(output_path);
        }
    }

    #[test]
    fn descriptor_heap_contract_requires_extension_and_capability() {
        let bytes = module(&[
            instruction(17, &[5_128]),
            string_instruction(10, "SPV_EXT_descriptor_heap"),
        ]);
        validate_descriptor_contract(&bytes, ShaderContract::descriptor_heap(16)).unwrap();
        assert!(validate_descriptor_contract(&bytes, ShaderContract::descriptor_free(16)).is_err());
    }

    #[test]
    fn every_contract_rejects_legacy_descriptor_decorations() {
        let bytes = module(&[instruction(71, &[1, 34, 0])]);
        assert!(validate_descriptor_contract(&bytes, ShaderContract::descriptor_free(16)).is_err());
    }

    #[test]
    fn every_contract_rejects_paired_set_and_binding_decorations() {
        let paired = module(&[
            instruction(71, &[1, 33, 0]),
            instruction(71, &[1, 34, 0]),
            instruction(71, &[2, 33, 3]),
            instruction(71, &[2, 34, 0]),
        ]);
        assert!(validate_descriptor_contract(&paired, ShaderContract::descriptor_free(0)).is_err());
        assert!(validate_descriptor_contract(&paired, ShaderContract::descriptor_heap(0)).is_err());
    }

    fn module(instructions: &[Vec<u32>]) -> Vec<u8> {
        [
            vec![0x0723_0203, 0x0001_0500, 0, 2, 0],
            instructions.concat(),
        ]
        .concat()
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect()
    }

    fn instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
        let word_count = u32::try_from(operands.len() + 1).unwrap();
        std::iter::once((word_count << 16) | opcode)
            .chain(operands.iter().copied())
            .collect()
    }

    fn string_instruction(opcode: u32, value: &str) -> Vec<u32> {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let operands = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        instruction(opcode, &operands)
    }

    fn buffer_pointer_storage_classes(bytes: &[u8]) -> Vec<u32> {
        let words = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        let mut pointer_classes = std::collections::BTreeMap::new();
        let mut buffer_pointer_types = Vec::new();
        let mut offset = 5;
        while offset < words.len() {
            let word_count = (words[offset] >> 16) as usize;
            let opcode = words[offset] & 0xffff;
            let operands = &words[offset + 1..offset + word_count];
            match opcode {
                OP_TYPE_POINTER | OP_TYPE_UNTYPED_POINTER_KHR => {
                    pointer_classes.insert(operands[0], operands[1]);
                }
                OP_BUFFER_POINTER_EXT => buffer_pointer_types.push(operands[0]),
                _ => {}
            }
            offset += word_count;
        }
        buffer_pointer_types
            .into_iter()
            .map(|id| pointer_classes[&id])
            .collect()
    }
}
