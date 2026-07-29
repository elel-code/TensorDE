//! Slang-owned source normalization for foreign shader frontends.
//!
//! The emitted syntax is HLSL-compatible Slang source. Callers may transform
//! reflected resource declarations into TensorDE's typed descriptor-heap ABI
//! before compiling it with [`crate::SlangCompiler::compile`].

use std::{ffi::OsString, fs, path::PathBuf};

use serde_json::Value;

use crate::{Error, Result, ShaderStage, SlangCompiler};

use super::compiler::run;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlslToSlangRequest {
    pub source: PathBuf,
    pub entry_point: String,
    pub stage: ShaderStage,
    pub output: PathBuf,
    pub include_directories: Vec<PathBuf>,
    pub definitions: Vec<(String, String)>,
    pub disabled_warnings: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlangSourceReport {
    pub output: PathBuf,
    pub source_bytes: usize,
    pub reflection: Value,
}

impl SlangCompiler {
    pub fn transpile_glsl(&self, request: &GlslToSlangRequest) -> Result<SlangSourceReport> {
        self.check_slang()?;
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::io("create shader source output directory", parent, error)
            })?;
        }
        let source_output = temporary_path(&request.output, "slang");
        let reflection_output = temporary_path(&request.output, "reflection.json");
        let result = self.run_glsl_frontend(request, &source_output, &reflection_output);
        if result.is_err() {
            let _ = fs::remove_file(&source_output);
            let _ = fs::remove_file(&reflection_output);
        }
        result?;
        let source_bytes = fs::metadata(&source_output)
            .map_err(|error| Error::io("stat normalized Slang source", &source_output, error))?
            .len() as usize;
        let reflection_bytes = fs::read(&reflection_output).map_err(|error| {
            Error::io("read Slang frontend reflection", &reflection_output, error)
        })?;
        let reflection = serde_json::from_slice(&reflection_bytes)
            .map_err(|error| Error::Reflection(format!("invalid frontend JSON: {error}")))?;
        fs::rename(&source_output, &request.output).map_err(|error| {
            Error::io("install normalized Slang source", &request.output, error)
        })?;
        let _ = fs::remove_file(&reflection_output);
        Ok(SlangSourceReport {
            output: request.output.clone(),
            source_bytes,
            reflection,
        })
    }

    fn run_glsl_frontend(
        &self,
        request: &GlslToSlangRequest,
        source_output: &std::path::Path,
        reflection_output: &std::path::Path,
    ) -> Result<()> {
        let mut arguments = vec![OsString::from("-lang"), OsString::from("glsl")];
        for directory in &request.include_directories {
            arguments.push(OsString::from("-I"));
            arguments.push(directory.as_os_str().to_owned());
        }
        for (name, value) in &request.definitions {
            arguments.push(OsString::from(format!("-D{name}={value}")));
        }
        arguments.extend([
            OsString::from("-entry"),
            request.entry_point.clone().into(),
            OsString::from("-stage"),
            request.stage.slang_name().into(),
            OsString::from("-target"),
            OsString::from("hlsl"),
            OsString::from("-profile"),
            OsString::from("glsl_450"),
            OsString::from("-matrix-layout-row-major"),
            OsString::from("-O2"),
            OsString::from("-warnings-as-errors"),
            OsString::from("all"),
        ]);
        if !request.disabled_warnings.is_empty() {
            arguments.push(OsString::from("-warnings-disable"));
            arguments.push(OsString::from(
                request
                    .disabled_warnings
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ));
        }
        arguments.extend([
            OsString::from("-restrictive-capability-check"),
            OsString::from("-reflection-json"),
            reflection_output.as_os_str().to_owned(),
            OsString::from("-o"),
            source_output.as_os_str().to_owned(),
            request.source.as_os_str().to_owned(),
        ]);
        run(&self.slangc, arguments).map(|_| ())
    }
}

fn temporary_path(output: &std::path::Path, suffix: &str) -> PathBuf {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shader");
    output.with_file_name(format!(".{name}.{}.tmp.{suffix}", std::process::id()))
}
