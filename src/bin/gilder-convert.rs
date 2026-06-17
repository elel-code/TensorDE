use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
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
        "  gilder-convert wallpaper-engine <source-project-dir> <dest.gwpdir>",
        "",
        "The initial converter supports static image, video, and web wallpapers. Scene projects are reported as unsupported for now.",
    ]
    .join("\n")
}
