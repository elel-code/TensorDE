use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/video/ffmpeg/shim.c");
    if env::var_os("CARGO_FEATURE_FFMPEG_VULKAN_DECODE").is_none() {
        return;
    }
    build_ffmpeg_vulkan_decode_shim();
}

fn build_ffmpeg_vulkan_decode_shim() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = output.join("ffmpeg_vulkan_decode_shim.o");
    let archive = output.join("libvulkan_renderer_ffmpeg_decode.a");
    let package = Command::new("pkg-config")
        .args([
            "--cflags",
            "--libs",
            "libavformat",
            "libavcodec",
            "libavutil",
        ])
        .output()
        .expect("run pkg-config for FFmpeg Vulkan decode");
    if !package.status.success() {
        panic!(
            "pkg-config FFmpeg Vulkan decode failed: {}",
            String::from_utf8_lossy(&package.stderr)
        );
    }
    let flags = String::from_utf8(package.stdout).expect("pkg-config output is UTF-8");
    let flags = flags.split_whitespace().collect::<Vec<_>>();
    let compile = Command::new("cc")
        .args([
            "-std=c11",
            "-fPIC",
            "-O2",
            "-ffunction-sections",
            "-fdata-sections",
            "-c",
        ])
        .args(flags.iter().copied().filter(|flag| {
            flag.starts_with("-I") || flag.starts_with("-D") || flag.starts_with("-f")
        }))
        .arg("src/video/ffmpeg/shim.c")
        .arg("-o")
        .arg(&object)
        .output()
        .expect("compile renderer-owned FFmpeg Vulkan decode shim");
    if !compile.status.success() {
        panic!(
            "compile renderer-owned FFmpeg Vulkan decode shim failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
    }
    let archive_result = Command::new("ar")
        .args(["crs"])
        .arg(&archive)
        .arg(&object)
        .output()
        .expect("archive renderer-owned FFmpeg Vulkan decode shim");
    if !archive_result.status.success() {
        panic!(
            "archive renderer-owned FFmpeg Vulkan decode shim failed: {}",
            String::from_utf8_lossy(&archive_result.stderr)
        );
    }
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=vulkan_renderer_ffmpeg_decode");
    println!("cargo:rustc-link-lib=dl");
    for flag in flags {
        if let Some(library) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={library}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
