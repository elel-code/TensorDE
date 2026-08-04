use super::*;

pub fn ingest_wallpaper_engine_project(
    project_root: impl AsRef<Path>,
) -> Result<WeSceneIr, WeIngestError> {
    let project_root = project_root.as_ref().to_path_buf();
    let source = WeAssetSource::open(project_root.clone())?;
    let project_asset = source.read_required_asset("project.json")?;
    let project_json = parse_json_bytes("project.json", &project_asset.bytes)?;
    let project = parse_project_ir(&project_json)?;
    if project.wallpaper_type != "scene" {
        return Err(WeIngestError::UnsupportedProjectType {
            wallpaper_type: project.wallpaper_type,
        });
    }
    let scene_asset = source.read_required_asset(&project.scene_file)?;
    let scene_json = parse_json_bytes(&project.scene_file, &scene_asset.bytes)?;
    let scene = parse_scene_root_ir(&scene_json);
    let font_overrides = text_font_overrides(&scene_json, &project_json).map_err(|message| {
        WeIngestError::Script {
            object: u32::MAX,
            message,
        }
    })?;
    let mut builder = WeIrBuilder::new(
        project_root,
        source,
        project,
        scene,
        project_property_defaults(&project_json),
        font_overrides,
        scene_effect_visibility_mutation_policy(&scene_json),
    );
    builder.add_existing_resource(
        "project.json",
        SceneResourceKind::ProjectJson,
        project_asset.source,
        project_asset.bytes,
    );
    builder.add_existing_resource(
        builder.project.scene_file.clone(),
        SceneResourceKind::SceneJson,
        scene_asset.source,
        scene_asset.bytes,
    );
    for (index, object) in scene_json
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        builder.ingest_object(index, object)?;
    }
    builder.finish()
}
