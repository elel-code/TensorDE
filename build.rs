use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_VIDEO").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out_dir.join("demux_ffmpeg_shim.o");
    let archive = out_dir.join("libgilder_demux_ffmpeg_shim.a");
    let source = PathBuf::from("src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");

    let pkg_config = Command::new("pkg-config")
        .args([
            "--cflags",
            "--libs",
            "libavformat",
            "libavcodec",
            "libavutil",
        ])
        .output()
        .expect("run pkg-config for FFmpeg");
    if !pkg_config.status.success() {
        panic!(
            "pkg-config libavformat/libavcodec/libavutil failed: {}",
            String::from_utf8_lossy(&pkg_config.stderr)
        );
    }
    let pkg_flags = String::from_utf8(pkg_config.stdout).expect("pkg-config output is UTF-8");
    let mut flags = pkg_flags.split_whitespace().collect::<Vec<_>>();

    let audio_cflags = Command::new("pkg-config")
        .args(["--cflags", "libpipewire-0.3", "libswresample"])
        .output()
        .expect("run pkg-config for audio cflags");
    if !audio_cflags.status.success() {
        panic!(
            "pkg-config libpipewire-0.3/libswresample --cflags failed: {}",
            String::from_utf8_lossy(&audio_cflags.stderr)
        );
    }
    let audio_cflags = String::from_utf8(audio_cflags.stdout).expect("pkg-config output is UTF-8");
    flags.extend(audio_cflags.split_whitespace());

    let mut cc = Command::new("cc");
    cc.args(["-std=c11", "-fPIC", "-O2", "-c"]);
    cc.args(
        flags.iter().copied().filter(|flag| {
            flag.starts_with("-I") || flag.starts_with("-D") || flag.starts_with("-f")
        }),
    );
    cc.arg(&source);
    cc.arg("-o");
    cc.arg(&object);
    let cc_output = cc.output().expect("compile FFmpeg demux shim");
    if !cc_output.status.success() {
        panic!(
            "compile FFmpeg demux shim failed: {}",
            String::from_utf8_lossy(&cc_output.stderr)
        );
    }

    let ar_output = Command::new("ar")
        .args(["crs"])
        .arg(&archive)
        .arg(&object)
        .output()
        .expect("archive FFmpeg demux shim");
    if !ar_output.status.success() {
        panic!(
            "archive FFmpeg demux shim failed: {}",
            String::from_utf8_lossy(&ar_output.stderr)
        );
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=gilder_demux_ffmpeg_shim");
    println!("cargo:rustc-link-lib=dl");
    for flag in flags {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
