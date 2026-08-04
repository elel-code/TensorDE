use std::env;
use std::fs::File;
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
            tensor_wallpaper::core::pack_gwp(&source, &dest)
                .map_err(|err| format!("failed to pack {}: {err}", source.display()))?;
            println!("packed {}", dest.display());
            Ok(())
        }
        [cmd, source, dest] if cmd == "unpack" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            tensor_wallpaper::core::unpack_gwp(&source, &dest)
                .map_err(|err| format!("failed to unpack {}: {err}", source.display()))?;
            println!("unpacked {}", dest.display());
            Ok(())
        }
        [cmd, source, dest] if cmd == "wallpaper-engine" => {
            let source = PathBuf::from(source);
            let dest = PathBuf::from(dest);
            let summary =
                tensor_wallpaper::convert::we_ingest::convert_wallpaper_engine_project_to_scene_binary(
                    &source, &dest,
                )
                .map_err(|err| {
                    format!(
                        "failed to convert Wallpaper Engine project {}: {err}",
                        source.display()
                    )
                })?;
            println!(
                "converted {} -> {} (objects={}, resources={}, materials={}, effects={}, meshes={} vertices={} indices={} source_records={} clipping_subdraws={} clipping_slices={}, puppets={} bones={} clips={} tracks={} transform_samples={} opacity_samples={}, object_transform_tracks={} channels={} keyframes={}, script_programs={}, graphs={}, shaders={}, shader_programs={}, shader_spirv={} bytes, heap_resources={}, heap_samplers={}, fifo_latest_ready={}, payload={} bytes)",
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
                summary.script_program_count,
                summary.render_graph_count,
                summary.shader_contract_count,
                summary.shader_program_count,
                summary.shader_spirv_bytes,
                summary.descriptor_heap_resource_count,
                summary.descriptor_heap_sampler_count,
                summary.fifo_latest_ready_present_required,
                summary.resource_payload_bytes
            );
            Ok(())
        }
        [cmd, source] if cmd == "inspect-scene" => inspect_scene(&PathBuf::from(source)),
        _ => Err(help_text()),
    }
}

fn inspect_scene(source: &std::path::Path) -> Result<(), String> {
    let input = File::open(source)
        .map_err(|error| format!("failed to open scene {}: {error}", source.display()))?;
    let storage = tensor_wallpaper::engine::scene::SceneStorage::from_binary_reader(input)
        .map_err(|error| format!("failed to validate scene {}: {error}", source.display()))?;
    println!(
        "scene-binary-version={}",
        tensor_wallpaper::engine::scene::SCENE_BINARY_VERSION
    );
    for program in storage.shader_programs() {
        let key = storage
            .string(program.program_key)
            .ok_or_else(|| "validated shader program has no key".to_owned())?;
        let entry = storage
            .string(program.entry_point)
            .ok_or_else(|| "validated shader program has no entry point".to_owned())?;
        println!(
            "shader-program key={key} stage={:?} entry={entry} spirv={} bytes bindings={} push={} bytes",
            program.stage,
            std::mem::size_of_val(storage.shader_program_spirv(program)),
            program.binding_count,
            program.push_constant_bytes,
        );
        for binding in storage.shader_program_bindings(program) {
            println!(
                "  shader-binding kind={:?} register={} descriptors={} push-offset={}",
                binding.kind, binding.register, binding.descriptor_count, binding.push_offset,
            );
        }
        for item in storage.shader_program_stage_io(program) {
            let name = storage
                .string(item.name)
                .ok_or_else(|| "validated shader stage-I/O has no name".to_owned())?;
            println!(
                "  shader-io direction={:?} location={} span={} name={name} type={:?}{}x{}",
                item.direction,
                item.location,
                item.location_count,
                item.scalar_type,
                item.rows,
                item.columns,
            );
        }
        for buffer in storage.shader_program_uniform_buffers(program) {
            let name = storage
                .string(buffer.name)
                .ok_or_else(|| "validated shader uniform buffer has no name".to_owned())?;
            println!(
                "  shader-uniform-buffer register={} bytes={} name={name} members={}",
                buffer.register, buffer.byte_size, buffer.member_count,
            );
            for member in storage.shader_uniform_buffer_members(buffer) {
                let name = storage
                    .string(member.name)
                    .ok_or_else(|| "validated shader uniform member has no name".to_owned())?;
                let material = storage
                    .string(member.material_parameter)
                    .map_or("<system>", |name| name);
                println!(
                    "    shader-uniform-member offset={} bytes={} name={name} material={material} type={:?}{}x{} array={} stride={} matrix-stride={}",
                    member.byte_offset,
                    member.byte_size,
                    member.scalar_type,
                    member.rows,
                    member.columns,
                    member.array_count,
                    member.array_stride,
                    member.matrix_stride,
                );
            }
        }
    }
    println!(
        "scene-shader-programs={} scene-shader-spirv={} bytes",
        storage.shader_programs().len(),
        storage.document().shader_spirv.len() * std::mem::size_of::<u32>(),
    );
    Ok(())
}

fn help_text() -> String {
    [
        "usage:",
        "  tensor-wallpaper-convert pack <source.gwpdir> <dest.gwp>",
        "  tensor-wallpaper-convert unpack <source.gwp> <dest.gwpdir>",
        "  tensor-wallpaper-convert wallpaper-engine <project-root> <dest.gscene>",
        "  tensor-wallpaper-convert inspect-scene <source.gscene>",
        "",
        "Wallpaper Engine scene conversion emits the new Tensor Wallpaper scene engine binary format.",
        "Pack accepts .gwpdir manifests in JSON or TOML and writes canonical JSON into .gwp archives.",
    ]
    .join("\n")
}
