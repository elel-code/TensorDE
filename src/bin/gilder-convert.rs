use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, source, dest] if cmd == "pack" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            gilder::core::pack_gwp(&source, &dest)
                .map_err(|err| format!("failed to pack {}: {err}", source.display()))?;
            println!("packed {}", dest.display());
            Ok(())
        }
        [cmd, source, dest] if cmd == "unpack" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            gilder::core::unpack_gwp(&source, &dest)
                .map_err(|err| format!("failed to unpack {}: {err}", source.display()))?;
            println!("unpacked {}", dest.display());
            Ok(())
        }
        [cmd, source, dest] if cmd == "image-gtex" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            if dest.exists() {
                return Err(format!("output texture already exists: {}", dest.display()));
            }
            let summary =
                gilder::convert::wallpaper_engine::convert_png_to_native_gtex(&source, &dest)?;
            print_native_gtex_summary(&summary);
            Ok(())
        }
        [kind, flag, source, dest] if kind == "wallpaper-engine" && flag == "--pack" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            if dest.exists() {
                return Err(format!("output archive already exists: {}", dest.display()));
            }
            let temp_dir = TempDir::new("gilder-convert-pack")?;
            let summary =
                gilder::convert::wallpaper_engine::convert_project(&source, temp_dir.path())
                    .map_err(|err| format!("failed to convert Wallpaper Engine project: {err}"))?;
            gilder::core::pack_gwp(temp_dir.path(), &dest)
                .map_err(|err| format!("failed to pack {}: {err}", dest.display()))?;
            println!(
                "converted Wallpaper Engine {} wallpaper",
                summary.source_type
            );
            println!("title: {}", summary.title);
            println!("archive: {}", dest.display());
            Ok(())
        }
        [kind, source, dest] if kind == "wallpaper-engine" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            let summary = gilder::convert::wallpaper_engine::convert_project(&source, &dest)
                .map_err(|err| format!("failed to convert Wallpaper Engine project: {err}"))?;
            println!(
                "converted Wallpaper Engine {} wallpaper",
                summary.source_type
            );
            println!("title: {}", summary.title);
            println!("output: {}", summary.output_dir.display());
            println!("manifest: {}", summary.manifest_file.display());
            println!("report: {}", summary.report_file.display());
            Ok(())
        }
        _ => Err(help_text()),
    }
}

fn help_text() -> String {
    [
        "usage:",
        "  gilder-convert pack <source.gwpdir> <dest.gwp>",
        "  gilder-convert unpack <source.gwp> <dest.gwpdir>",
        "  gilder-convert image-gtex <source.png> <dest.gtex>",
        "  gilder-convert wallpaper-engine <source-project-dir> <dest.gwpdir>",
        "  gilder-convert wallpaper-engine --pack <source-project-dir> <dest.gwp>",
        "",
        "The converter supports static image, video, web, and first-class scene Wallpaper Engine projects.",
        "Video projects without previews use ffmpeg from PATH for first-frame poster generation when available.",
        "Pack accepts .gwpdir manifests in JSON or TOML and writes canonical JSON into .gwp archives.",
    ]
    .join("\n")
}

fn print_native_gtex_summary(
    summary: &gilder::convert::wallpaper_engine::NativeGtexConversionSummary,
) {
    println!("converted image to native gtex");
    println!("source: {}", summary.source.display());
    println!("output: {}", summary.output.display());
    println!("size: {}x{}", summary.width, summary.height);
    println!("format: {}", summary.format);
    println!("payload_bytes: {}", summary.payload_bytes);
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|err| format!("failed to create temp directory {}: {err}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
