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
            println!(
                "planned conversion: Wallpaper Engine project {} -> {}",
                source.display(),
                dest.display()
            );
            println!(
                "output format: Gilder v{} ({})",
                gilder::core::FORMAT_VERSION,
                gilder::core::MANIFEST_FILE
            );
            println!("status: scanner and asset transcoders are tracked in docs/todo.md");
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
        "The initial converter will support static image, video, web, and a subset of scene wallpapers.",
    ]
    .join("\n")
}
