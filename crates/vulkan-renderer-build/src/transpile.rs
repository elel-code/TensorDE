//! Slang-owned source normalization for foreign shader frontends.
//!
//! The emitted syntax is HLSL-compatible Slang source. Callers may transform
//! reflected resource declarations into TensorDE's typed descriptor-heap ABI
//! before compiling it with [`crate::SlangCompiler::compile`].

use std::{collections::BTreeMap, ffi::OsString, fs, path::PathBuf};

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
    pub fn preprocess_glsl(&self, request: &GlslToSlangRequest) -> Result<String> {
        self.check_slang()?;
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
            OsString::from("-E"),
            request.source.as_os_str().to_owned(),
        ]);
        let output = run(&self.slangc, arguments)?;
        String::from_utf8(output.stdout).map_err(|error| {
            Error::SourceLowering(format!("Slang preprocessor output is not UTF-8: {error}"))
        })
    }

    pub fn transpile_glsl(&self, request: &GlslToSlangRequest) -> Result<SlangSourceReport> {
        self.check_slang()?;
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::io("create shader source output directory", parent, error)
            })?;
        }
        let source_output = temporary_path(&request.output, "slang");
        let reflection_output = temporary_path(&request.output, "reflection.json");
        let reflected_source_output = temporary_path(&request.output, "reflected.hlsl");
        let result = self
            .run_glsl_frontend(request, &source_output, &reflection_output)
            .and_then(|()| preserve_explicit_glsl_locations(request, &source_output))
            .and_then(|()| {
                self.run_normalized_reflection(
                    request,
                    &source_output,
                    &reflected_source_output,
                    &reflection_output,
                )
            });
        if result.is_err() {
            let _ = fs::remove_file(&source_output);
            let _ = fs::remove_file(&reflection_output);
            let _ = fs::remove_file(&reflected_source_output);
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
        let _ = fs::remove_file(&reflected_source_output);
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
            OsString::from("-no-mangle"),
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

    fn run_normalized_reflection(
        &self,
        request: &GlslToSlangRequest,
        source: &std::path::Path,
        output: &std::path::Path,
        reflection_output: &std::path::Path,
    ) -> Result<()> {
        let mut disabled_warnings = request.disabled_warnings.clone();
        disabled_warnings.push(15601);
        disabled_warnings.sort_unstable();
        disabled_warnings.dedup();
        let mut arguments = vec![
            OsString::from("-lang"),
            OsString::from("hlsl"),
            source.as_os_str().to_owned(),
            OsString::from("-entry"),
            request.entry_point.clone().into(),
            OsString::from("-stage"),
            request.stage.slang_name().into(),
            OsString::from("-target"),
            OsString::from("hlsl"),
            OsString::from("-profile"),
            OsString::from("sm_6_0"),
            OsString::from("-matrix-layout-row-major"),
            OsString::from("-O2"),
            OsString::from("-warnings-as-errors"),
            OsString::from("all"),
            OsString::from("-warnings-disable"),
            OsString::from(
                disabled_warnings
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            OsString::from("-restrictive-capability-check"),
            OsString::from("-reflection-json"),
            reflection_output.as_os_str().to_owned(),
            OsString::from("-o"),
            output.as_os_str().to_owned(),
        ];
        run(&self.slangc, arguments.drain(..)).map(|_| ())
    }
}

fn temporary_path(output: &std::path::Path, suffix: &str) -> PathBuf {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shader");
    output.with_file_name(format!(".{name}.{}.tmp.{suffix}", std::process::id()))
}

fn preserve_explicit_glsl_locations(
    request: &GlslToSlangRequest,
    normalized_hlsl: &std::path::Path,
) -> Result<()> {
    let glsl = fs::read_to_string(&request.source)
        .map_err(|error| Error::io("read GLSL location source", &request.source, error))?;
    let locations = explicit_glsl_locations(&glsl)?;
    if locations.is_empty() {
        return Ok(());
    }
    let hlsl = fs::read_to_string(normalized_hlsl)
        .map_err(|error| Error::io("read normalized HLSL locations", normalized_hlsl, error))?;
    let normalized = annotate_hlsl_vulkan_locations(hlsl, &locations);
    fs::write(normalized_hlsl, normalized)
        .map_err(|error| Error::io("write normalized HLSL locations", normalized_hlsl, error))
}

fn explicit_glsl_locations(source: &str) -> Result<BTreeMap<String, u32>> {
    let mut locations = BTreeMap::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(layout) = line.strip_prefix("layout(") else {
            continue;
        };
        let Some((layout, declaration)) = layout.split_once(')') else {
            return Err(Error::SourceLowering(format!(
                "explicit GLSL stage-I/O layout has no closing parenthesis: {line}"
            )));
        };
        let Some(location) = layout
            .split(',')
            .find_map(|qualifier| qualifier.trim().strip_prefix("location"))
            .and_then(|value| value.trim_start().strip_prefix('='))
            .map(str::trim)
        else {
            continue;
        };
        let location = location.parse::<u32>().map_err(|error| {
            Error::SourceLowering(format!(
                "explicit GLSL stage-I/O location is not a u32 in `{line}`: {error}"
            ))
        })?;
        let declaration = declaration.trim();
        let stage_io = declaration
            .strip_prefix("in ")
            .or_else(|| declaration.strip_prefix("out "));
        let Some(stage_io) = stage_io else {
            continue;
        };
        let declarator = stage_io
            .strip_suffix(';')
            .and_then(|declaration| declaration.split_whitespace().next_back())
            .ok_or_else(|| {
                Error::SourceLowering(format!(
                    "explicit GLSL stage-I/O declaration has no identifier: {line}"
                ))
            })?;
        let name = declarator
            .split_once('[')
            .map_or(declarator, |(name, _)| name);
        if name.is_empty() || !name.bytes().all(is_identifier_byte) {
            return Err(Error::SourceLowering(format!(
                "explicit GLSL stage-I/O identifier is invalid in `{line}`"
            )));
        }
        if let Some(previous) = locations.insert(name.to_owned(), location)
            && previous != location
        {
            return Err(Error::SourceLowering(format!(
                "explicit GLSL stage-I/O {name} repeats locations {previous} and {location}"
            )));
        }
    }
    Ok(locations)
}

fn annotate_hlsl_vulkan_locations(mut source: String, locations: &BTreeMap<String, u32>) -> String {
    for (name, location) in locations {
        let needle = format!("{name} : ");
        let mut search_start = 0;
        while let Some(relative) = source[search_start..].find(&needle) {
            let name_start = search_start + relative;
            let semantic_start = name_start + needle.len();
            if name_start != 0
                && source[..name_start]
                    .bytes()
                    .next_back()
                    .is_some_and(is_identifier_byte)
            {
                search_start = semantic_start;
                continue;
            }
            let declaration_start = source[..name_start]
                .rfind(['\n', '{', '(', ',', ';'])
                .map_or(0, |delimiter| delimiter + 1);
            let declaration_start = source[declaration_start..name_start]
                .find(|character: char| !character.is_whitespace())
                .map_or(name_start, |offset| declaration_start + offset);
            let attribute = format!("[[vk::location({location})]] ");
            source.insert_str(declaration_start, &attribute);
            search_start = semantic_start + attribute.len();
        }
    }
    source
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_explicit_glsl_locations_after_frontend_reorders_hlsl_semantics() {
        let locations = explicit_glsl_locations(
            "layout(location = 0) out vec4 first;\n\
             layout(location = 1) out vec3 second;\n\
             layout(location = 2) out vec4 third;",
        )
        .expect("explicit locations");
        let reordered = "struct Output { float4 third : COLOR0; float4 first : COLOR1; \
                         float3 second : COLOR2; };\n\
                         Output main(float4 first : VERTEX_IN_3)";

        let normalized = annotate_hlsl_vulkan_locations(reordered.to_owned(), &locations);

        assert!(normalized.contains("[[vk::location(2)]] float4 third : COLOR0"));
        assert!(normalized.contains("[[vk::location(0)]] float4 first : COLOR1"));
        assert!(normalized.contains("[[vk::location(1)]] float3 second : COLOR2"));
        assert!(normalized.contains("main([[vk::location(0)]] float4 first : VERTEX_IN_3)"));
    }

    #[test]
    fn rejects_conflicting_explicit_locations_for_one_identifier() {
        let error = explicit_glsl_locations(
            "layout(location = 1) in vec2 uv;\nlayout(location = 2) out vec2 uv;",
        )
        .expect_err("one identifier cannot own two semantic indices");
        assert!(error.to_string().contains("repeats locations 1 and 2"));
    }
}
