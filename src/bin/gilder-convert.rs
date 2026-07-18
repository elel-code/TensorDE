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
        [cmd, source, dest] if cmd == "wallpaper-engine" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            let summary =
                gilder::convert::we_ingest::convert_wallpaper_engine_project_to_scene_binary(
                    &source, &dest,
                )
                .map_err(|err| {
                    format!(
                        "failed to convert Wallpaper Engine project {}: {err}",
                        source.display()
                    )
                })?;
            println!(
                "converted {} -> {} (objects={}, resources={}, materials={}, effects={}, meshes={} vertices={} indices={} source_records={} clipping_subdraws={} clipping_slices={}, puppets={} bones={} clips={} tracks={} transform_samples={} opacity_samples={}, object_transform_tracks={} channels={} keyframes={}, text_providers={}, script_programs={}, graphs={}, shaders={}, heap_resources={}, heap_samplers={}, fifo_latest_ready={}, payload={} bytes)",
                source.display(),
                dest.display(),
                summary.object_count,
                summary.resource_count,
                summary.material_count,
                summary.effect_count,
                summary.mesh_count,
                summary.mesh_vertex_count,
                summary.mesh_index_count,
                summary.mesh_source_record_count,
                summary.mesh_clipping_subdraw_count,
                summary.mesh_clipping_slice_count,
                summary.puppet_count,
                summary.puppet_bone_count,
                summary.puppet_animation_clip_count,
                summary.puppet_animation_track_count,
                summary.puppet_animation_transform_sample_count,
                summary.puppet_animation_opacity_sample_count,
                summary.object_transform_track_count,
                summary.object_transform_channel_count,
                summary.object_transform_keyframe_count,
                summary.text_provider_count,
                summary.script_program_count,
                summary.render_graph_count,
                summary.shader_contract_count,
                summary.descriptor_heap_resource_count,
                summary.descriptor_heap_sampler_count,
                summary.fifo_latest_ready_present_required,
                summary.resource_payload_bytes
            );
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
        "  gilder-convert wallpaper-engine <project-root> <dest.gscene>",
        "",
        "Wallpaper Engine scene conversion emits the new Gilder scene engine binary format.",
        "Pack accepts .gwpdir manifests in JSON or TOML and writes canonical JSON into .gwp archives.",
    ]
    .join("\n")
}
