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
        _ => Err(help_text()),
    }
}

fn help_text() -> String {
    [
        "usage:",
        "  gilder-convert pack <source.gwpdir> <dest.gwp>",
        "  gilder-convert unpack <source.gwp> <dest.gwpdir>",
        "",
        "Wallpaper Engine conversion is removed until the new Gilder scene engine binary format is defined.",
        "Pack accepts .gwpdir manifests in JSON or TOML and writes canonical JSON into .gwp archives.",
    ]
    .join("\n")
}
