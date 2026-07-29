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
    VULKAN_TARGET_ENVIRONMENT,
};

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlangCompiler {
    slangc: PathBuf,
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
        self.check_tools()?;
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| Error::io("create shader output directory", parent, error))?;
        }
        let temporary = TemporaryOutputs::new(&request.output);
        self.run_slang(request, &temporary)?;
        request.contract.validate(
            &read_reflection(&temporary.reflection)?,
            &request.entry_point,
            request.stage,
        )?;
        self.validate_spirv(&temporary.spirv)?;
        let bytes = fs::read(&temporary.spirv)
            .map_err(|error| Error::io("read generated SPIR-V", &temporary.spirv, error))?;
        validate_word_length(&bytes)?;
        validate_descriptor_contract(&bytes, request.contract)?;
        fs::rename(&temporary.spirv, &request.output)
            .map_err(|error| Error::io("install generated SPIR-V", &request.output, error))?;
        Ok(CompileReport {
            output: request.output.clone(),
            spirv_bytes: bytes.len(),
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
        })
    }

    fn check_tools(&self) -> Result<()> {
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
        run(&self.spirv_val, [OsString::from("--version")])?;
        Ok(())
    }

    fn run_slang(
        &self,
        request: &ShaderCompileRequest,
        temporary: &TemporaryOutputs,
    ) -> Result<()> {
        let mut arguments = vec![
            request.source.as_os_str().to_owned(),
            "-entry".into(),
            request.entry_point.clone().into(),
            "-stage".into(),
            request.stage.slang_name().into(),
            "-target".into(),
            "spirv".into(),
            "-profile".into(),
            SPIRV_PROFILE.into(),
            "-std".into(),
            "2026".into(),
            "-matrix-layout-row-major".into(),
            "-O2".into(),
            "-warnings-as-errors".into(),
            "all".into(),
            "-restrictive-capability-check".into(),
            "-emit-spirv-directly".into(),
            "-reflection-json".into(),
            temporary.reflection.as_os_str().to_owned(),
            "-o".into(),
            temporary.spirv.as_os_str().to_owned(),
        ];
        if request.contract.emits_native_descriptor_heap() {
            let output_position = arguments.len() - 2;
            arguments.splice(
                output_position..output_position,
                [
                    OsString::from("-capability"),
                    OsString::from("spvDescriptorHeapEXT"),
                ],
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

fn environment_tool(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn run(tool: &Path, arguments: impl IntoIterator<Item = OsString>) -> Result<Output> {
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

    let mapped_bindings = !binding_ids.is_empty() || !descriptor_set_ids.is_empty();
    if contract.uses_mapped_descriptor_heap() {
        if binding_ids.is_empty() || binding_ids != descriptor_set_ids {
            return Err(Error::SpirvContract(
                "mapped descriptor-heap SPIR-V requires matching Binding and DescriptorSet decorations"
                    .to_owned(),
            ));
        }
        if descriptor_heap_capability || descriptor_heap_extension {
            return Err(Error::SpirvContract(
                "mapped descriptor-heap SPIR-V unexpectedly uses native DescriptorHeapEXT"
                    .to_owned(),
            ));
        }
        return Ok(());
    }
    if mapped_bindings {
        return Err(Error::SpirvContract(
            "SPIR-V contains Binding or DescriptorSet decorations outside the mapped descriptor-heap contract"
                .to_owned(),
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
    fn mapped_descriptor_heap_requires_paired_set_and_binding_decorations() {
        let paired = module(&[
            instruction(71, &[1, 33, 0]),
            instruction(71, &[1, 34, 0]),
            instruction(71, &[2, 33, 3]),
            instruction(71, &[2, 34, 0]),
        ]);
        validate_descriptor_contract(&paired, ShaderContract::mapped_descriptor_heap(0)).unwrap();
        assert!(validate_descriptor_contract(&paired, ShaderContract::descriptor_free(0)).is_err());

        let unpaired = module(&[instruction(71, &[1, 33, 0])]);
        assert!(
            validate_descriptor_contract(&unpaired, ShaderContract::mapped_descriptor_heap(0))
                .is_err()
        );
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
}
