use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=shaders/client.vert");
    println!("cargo:rerun-if-changed=shaders/client.frag");
    println!("cargo:rerun-if-changed=shaders/cursor.vert");
    println!("cargo:rerun-if-changed=shaders/cursor.frag");
    println!("cargo:rerun-if-changed=shaders/focus_ring.vert");
    println!("cargo:rerun-if-changed=shaders/focus_ring.frag");

    if env::var_os("CARGO_FEATURE_TTY").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo always sets OUT_DIR"));
    for (source, stage, output) in [
        ("shaders/client.vert", "vert", "tensor_client.vert.spv"),
        ("shaders/client.frag", "frag", "tensor_client.frag.spv"),
        ("shaders/cursor.vert", "vert", "tensor_cursor.vert.spv"),
        ("shaders/cursor.frag", "frag", "tensor_cursor.frag.spv"),
        (
            "shaders/focus_ring.vert",
            "vert",
            "tensor_focus_ring.vert.spv",
        ),
        (
            "shaders/focus_ring.frag",
            "frag",
            "tensor_focus_ring.frag.spv",
        ),
    ] {
        let destination = out_dir.join(output);
        let result = Command::new("glslangValidator")
            .args(["-V", "--target-env", "vulkan1.4", "-S", stage, "-o"])
            .arg(&destination)
            .arg(source)
            .output()
            .unwrap_or_else(|error| {
                panic!("run glslangValidator for {source} (install Vulkan shader tools): {error}")
            });
        if !result.status.success() {
            panic!(
                "compile {source} failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let size = fs::metadata(&destination)
            .unwrap_or_else(|error| panic!("stat generated shader {destination:?}: {error}"))
            .len();
        assert!(
            size >= 4 && size % 4 == 0,
            "generated shader has invalid size"
        );
    }
}
