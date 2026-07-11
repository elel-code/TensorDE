use std::env;
use std::path::PathBuf;
use std::process::Command;

pub(super) fn build_system_audio_monitor() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out_dir.join("system_audio_monitor_pipewire.o");
    let archive = out_dir.join("libgilder_system_audio_monitor.a");
    let source =
        PathBuf::from("src/renderer/native_vulkan/audio/system_monitor/pipewire_monitor.c");
    let pkg_config = Command::new("pkg-config")
        .args(["--cflags", "--libs", "libpipewire-0.3"])
        .output()
        .expect("run pkg-config for PipeWire system audio monitor");
    if !pkg_config.status.success() {
        panic!(
            "pkg-config libpipewire-0.3 failed for system audio monitor: {}",
            String::from_utf8_lossy(&pkg_config.stderr)
        );
    }
    let flags = String::from_utf8(pkg_config.stdout).expect("pkg-config output is UTF-8");
    let flags = flags.split_whitespace().collect::<Vec<_>>();
    let mut cc = Command::new("cc");
    cc.args([
        "-std=c11",
        "-D_GNU_SOURCE",
        "-fPIC",
        "-O2",
        "-ffunction-sections",
        "-fdata-sections",
        "-c",
    ]);
    cc.args(
        flags.iter().copied().filter(|flag| {
            flag.starts_with("-I") || flag.starts_with("-D") || flag.starts_with("-f")
        }),
    );
    cc.arg(&source).arg("-o").arg(&object);
    let output = cc.output().expect("compile PipeWire system audio monitor");
    if !output.status.success() {
        panic!(
            "compile PipeWire system audio monitor failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new("ar")
        .args(["crs"])
        .arg(&archive)
        .arg(&object)
        .output()
        .expect("archive PipeWire system audio monitor");
    if !output.status.success() {
        panic!(
            "archive PipeWire system audio monitor failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=gilder_system_audio_monitor");
    println!("cargo:rustc-link-lib=m");
    for flag in flags {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
