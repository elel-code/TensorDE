use std::{env, fs, path::PathBuf, process::ExitCode};

use vulkan_renderer_build::{
    ShaderCompileRequest, ShaderContract, ShaderStage, SlangCompiler,
    lower_slang_bindings_to_descriptor_heap,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let mode = arguments.next().ok_or_else(usage)?;
    if mode == "lower-heap" {
        return lower_heap(arguments);
    }
    let source = PathBuf::from(arguments.next().ok_or_else(usage)?);
    let entry_point = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    let stage = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?
        .parse::<ShaderStage>()
        .map_err(|error| error.to_string())?;
    let output = PathBuf::from(arguments.next().ok_or_else(usage)?);
    let push_constant_bytes = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?
        .parse::<u64>()
        .map_err(|error| format!("invalid push-constant byte size: {error}"))?;
    let descriptor_mode = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }

    let contract = match descriptor_mode.as_str() {
        "descriptor-free" => ShaderContract::descriptor_free(push_constant_bytes),
        "descriptor-heap" => ShaderContract::descriptor_heap(push_constant_bytes),
        _ => return Err(usage()),
    };

    let request = ShaderCompileRequest {
        source,
        entry_point,
        stage,
        output,
        contract,
    };
    let compiler = SlangCompiler::from_environment();
    let report = match mode.to_str() {
        Some("compile") => compiler.compile(&request),
        Some("verify") => compiler.verify(&request),
        _ => return Err(usage()),
    }
    .map_err(|error| error.to_string())?;
    println!("{}: {} bytes", report.output.display(), report.spirv_bytes);
    Ok(())
}

fn lower_heap(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    let source = PathBuf::from(arguments.next().ok_or_else(usage)?);
    let entry_point = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    let output = PathBuf::from(arguments.next().ok_or_else(usage)?);
    if arguments.next().is_some() {
        return Err(usage());
    }
    let source_text = fs::read_to_string(&source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?;
    let lowered = lower_slang_bindings_to_descriptor_heap(&source_text, &entry_point)
        .map_err(|error| error.to_string())?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&output, lowered.source)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    println!(
        "{}: {} push bytes, {} bindings",
        output.display(),
        lowered.push_constant_bytes,
        lowered.bindings.len()
    );
    for binding in lowered.bindings {
        println!(
            "{:?} register={} push-offset={} name={}",
            binding.kind, binding.register, binding.push_offset, binding.source_name
        );
    }
    Ok(())
}

fn usage() -> String {
    "usage:\n  vulkan-renderer-build <compile|verify> <source> <entry> <vertex|fragment|compute> <output.spv> <push-constant-bytes> <descriptor-free|descriptor-heap>\n  vulkan-renderer-build lower-heap <source.slang> <entry> <output.slang>".to_owned()
}
