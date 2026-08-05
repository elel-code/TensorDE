use super::*;

struct EffectMaterialInstanceInput<'a> {
    object: u32,
    base_material: WeIrMaterial,
    base_pass: WeIrMaterialPass,
    base_textures: Vec<WeIrMaterialTexture>,
    instance_pass: Option<&'a Value>,
    resolved_bindings: &'a BTreeMap<u32, String>,
    shader_key: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;

    fn scene_package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(&8u32.to_le_bytes());
        header.extend_from_slice(b"PKGV0024");
        header.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut data = Vec::new();
        for (path, payload) in entries {
            header.extend_from_slice(&(path.len() as u32).to_le_bytes());
            header.extend_from_slice(path.as_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            data.extend_from_slice(payload);
        }
        header.extend(data);
        header
    }

    #[test]
    fn sampler_combo_uses_explicit_binding_not_shader_default() {
        let root = std::env::temp_dir().join(format!(
            "tensor-wallpaper-we-sampler-combo-binding-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test directory");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json","title":"Sampler combo"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.pkg"),
            scene_package(&[
                (
                    "scene.json",
                    br#"{"objects":[{"id":1,"image":"models/layer.json","effects":[{"file":"effects/mask/effect.json"}]},{"id":2,"image":"models/layer.json","effects":[{"file":"effects/mask/effect.json","passes":[{"textures":[null,null,"_rt_authored_mask"]}]}]}]}"#,
                ),
                (
                    "models/layer.json",
                    br#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
                ),
                (
                    "materials/layer.json",
                    br#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
                ),
                (
                    "effects/mask/effect.json",
                    br#"{"passes":[{"material":"materials/effects/mask.json"}]}"#,
                ),
                (
                    "materials/effects/mask.json",
                    br#"{"passes":[{"shader":"workshop/test/effects/mask"}]}"#,
                ),
                (
                    "shaders/workshop/test/effects/mask.vert",
                    br#"attribute vec3 a_Position;"#,
                ),
                (
                    "shaders/workshop/test/effects/mask.frag",
                    concat!(
                        "uniform sampler2D g_Texture0; // {\"material\":\"previous\"}\n",
                        "uniform sampler2D g_Texture2; // {\"combo\":\"OPACITYMASK\",\"default\":\"_rt_default_mask\"}"
                    )
                    .as_bytes(),
                ),
            ]),
        )
        .expect("scene package");

        let ir = ingest_wallpaper_engine_project(&root).expect("effect IR");
        let keys = ir
            .shader_contracts
            .iter()
            .map(|contract| contract.shader_key.as_str())
            .collect::<BTreeSet<_>>();
        assert!(keys.contains("workshop/test/effects/mask__SLOTS_5"));
        assert!(keys.contains("workshop/test/effects/mask__SLOTS_5__OPACITYMASK_1"));

        let _ = fs::remove_dir_all(root);
    }
}

impl WeIrBuilder {
    pub(super) fn push_effect_contracts_for_instance(
        &mut self,
        object: u32,
        effect_binding_start: u32,
        effect_handle: u32,
        instance: &Value,
        runtime_visibility: bool,
        out: &mut Vec<WeEffectPassContract>,
    ) -> Result<(), WeIngestError> {
        let Some(effect) = self.effects.get(effect_handle as usize).cloned() else {
            return Ok(());
        };
        let effect_file = self
            .resources
            .get(effect.resource as usize)
            .map(|resource| resource.path.clone())
            .unwrap_or_default();
        for local_index in 0..effect.pass_count {
            let pass_index = effect.pass_start + local_index;
            let Some(effect_pass) = self.effect_passes.get(pass_index as usize).cloned() else {
                continue;
            };
            let base_material = effect_pass
                .material
                .and_then(|material| self.materials.get(material as usize))
                .cloned();
            let material_pass = base_material
                .as_ref()
                .and_then(|material| self.material_passes.get(material.pass_start as usize))
                .cloned();
            let base_textures = material_pass
                .as_ref()
                .map(|pass| {
                    self.material_textures
                        .iter()
                        .skip(pass.texture_start as usize)
                        .take(pass.texture_count as usize)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut binds = BTreeMap::new();
            for binding in self
                .effect_bindings
                .iter()
                .skip(effect_pass.binding_start as usize)
                .take(effect_pass.binding_count as usize)
            {
                binds.insert(binding.slot, binding.target.clone());
            }
            material_texture_bindings(&base_textures, &mut binds);
            let instance_pass = instance
                .get("passes")
                .and_then(Value::as_array)
                .and_then(|passes| passes.get(local_index as usize));
            if let Some(instance_pass) = instance_pass {
                push_instance_texture_overrides(instance_pass, &mut binds);
            }
            if material_pass.is_some() {
                binds.entry(0).or_insert_with(|| "previous".to_owned());
            }
            let mut combos = BTreeMap::new();
            for combo in self
                .effect_combos
                .iter()
                .skip(effect_pass.combo_start as usize)
                .take(effect_pass.combo_count as usize)
            {
                combos.insert(combo.name.clone(), combo.value);
            }
            if let Some(instance_pass) = instance_pass {
                push_instance_combo_overrides(instance_pass, &mut combos);
            }
            let base_shader_source = material_pass
                .as_ref()
                .map(|pass| pass.shader_source_key.clone())
                .filter(|shader| !shader.is_empty());
            let base_shader_program = material_pass
                .as_ref()
                .map(material_shader_program_base)
                .filter(|shader| !shader.is_empty());
            let combo_defaults = base_shader_source
                .as_deref()
                .map(|shader| self.shader_combo_defaults(shader))
                .transpose()?
                .unwrap_or_default();
            if material_pass
                .as_ref()
                .is_some_and(|pass| pass.shader_origin == WeIrShaderOrigin::AuthoredPackage)
                && let Some(base_shader) = base_shader_source.as_deref()
            {
                for sampler_combo in self.shader_sampler_combos(base_shader)? {
                    combos
                        .entry(sampler_combo.name)
                        .or_insert_with(|| i64::from(binds.contains_key(&sampler_combo.slot)));
                }
            }
            apply_builtin_effect_texture_defaults(&effect_file, &combos, &mut binds);
            if let Some(base_shader) = base_shader_source.as_deref() {
                let shader_defaults = self.shader_texture_defaults(base_shader)?;
                apply_shader_texture_defaults(
                    &shader_defaults,
                    &combos,
                    &combo_defaults,
                    &mut binds,
                );
            }
            let shader = base_shader_program
                .as_deref()
                .map(|shader| effect_shader_variant_key(shader, &binds, &combos, &combo_defaults));
            let mut resolved_shader = shader.clone();
            let (material_index, pass_constants) =
                match (base_material, material_pass.clone(), shader.as_deref()) {
                    (Some(material), Some(pass), Some(shader)) => {
                        let (material, material_shader) =
                            self.add_effect_material_instance(EffectMaterialInstanceInput {
                                object,
                                base_material: material,
                                base_pass: pass,
                                base_textures,
                                instance_pass,
                                resolved_bindings: &binds,
                                shader_key: shader,
                            })?;
                        resolved_shader = Some(material_shader);
                        let pass = self.materials.get(material as usize).and_then(|material| {
                            self.material_passes.get(material.pass_start as usize)
                        });
                        (
                            Some(material as usize),
                            material_pass_constant_names(&self.material_constants, pass),
                        )
                    }
                    _ => (
                        effect_pass.material.map(|material| material as usize),
                        Vec::new(),
                    ),
                };
            out.push(WeEffectPassContract {
                object_index: object as usize,
                effect_binding_start,
                effect_binding_count: 1,
                runtime_visibility,
                material_index,
                effect_file: effect_file.clone(),
                pass_index: local_index,
                command: non_empty_string(&effect_pass.command),
                shader: resolved_shader,
                source: non_empty_string(&effect_pass.source),
                target: if effect_pass.target.is_empty() {
                    None
                } else {
                    Some(effect_pass.target.clone())
                },
                binds,
                pass_constants,
                material_blending: material_pass
                    .as_ref()
                    .map(|pass| pipeline_blend_string(pass.pipeline_blend)),
                depthtest: material_pass.as_ref().map(|pass| match pass.depth_test {
                    SceneDepthTest::Enabled => "enabled".to_owned(),
                    SceneDepthTest::Disabled => "disabled".to_owned(),
                }),
                depthwrite: material_pass.as_ref().map(|pass| {
                    if pass.depth_write {
                        "enabled".to_owned()
                    } else {
                        "disabled".to_owned()
                    }
                }),
                cullmode: material_pass.as_ref().map(|pass| match pass.cull_mode {
                    SceneCullMode::Normal => "normal".to_owned(),
                    SceneCullMode::None => "nocull".to_owned(),
                }),
                combos,
            });
        }
        Ok(())
    }

    fn add_effect_material_instance(
        &mut self,
        input: EffectMaterialInstanceInput<'_>,
    ) -> Result<(u32, String), WeIngestError> {
        let EffectMaterialInstanceInput {
            object,
            base_material,
            base_pass,
            base_textures,
            instance_pass,
            resolved_bindings,
            shader_key,
        } = input;
        let material_path = self
            .resources
            .get(base_material.resource as usize)
            .map(|resource| resource.path.clone());
        let mut textures = base_textures
            .into_iter()
            .map(|texture| (texture.slot, texture))
            .collect::<BTreeMap<_, _>>();
        for (slot, path) in file_texture_bindings(resolved_bindings) {
            let resource = self.add_texture(&path, material_path.as_deref())?;
            textures.insert(
                slot,
                WeIrMaterialTexture {
                    slot,
                    resource,
                    path,
                },
            );
        }
        let base_constants = self
            .material_constants
            .iter()
            .skip(base_pass.constant_start as usize)
            .take(base_pass.constant_count as usize)
            .cloned()
            .collect::<Vec<_>>();
        let constants = merged_material_constants(&base_constants, instance_pass);
        let textures = textures.into_values().collect::<Vec<_>>();
        let shader_key =
            caustics_specialization::specialize_caustics_shader(shader_key, &constants, &textures);
        let handle = self.materials.len() as u32;
        let texture_start = self.material_textures.len() as u32;
        self.material_textures.extend(textures);
        let constant_start = self.material_constants.len() as u32;
        self.material_constants.extend(constants.iter().cloned());
        let material_scripts = material_scalar_script_programs(
            object,
            constant_start,
            &constants,
            instance_pass,
            &self.project_property_defaults,
        )
        .map_err(|source| WeIngestError::Script {
            object,
            message: source.to_string(),
        })?;
        self.script_programs.extend(material_scripts);
        let mut pass = base_pass;
        pass.material = handle;
        pass.shader_key = shader_key.clone();
        pass.texture_start = texture_start;
        pass.texture_count = self.material_textures.len() as u32 - texture_start;
        pass.constant_start = constant_start;
        pass.constant_count = self.material_constants.len() as u32 - constant_start;
        let pass_start = self.material_passes.len() as u32;
        self.material_passes.push(pass);
        self.materials.push(WeIrMaterial {
            handle,
            resource: base_material.resource,
            pass_start,
            pass_count: 1,
        });
        Ok((handle, shader_key))
    }

    fn shader_sampler_combos(
        &self,
        shader_key: &str,
    ) -> Result<Vec<ShaderSamplerCombo>, WeIngestError> {
        let shader_key = shader_key.split("__").next().unwrap_or(shader_key);
        let mut combos = Vec::new();
        for extension in ["vert", "frag"] {
            let path = format!("shaders/{shader_key}.{extension}");
            let Some(asset) = self.source.read_optional_asset(&path)? else {
                continue;
            };
            let source = String::from_utf8_lossy(&asset.bytes);
            combos.extend(
                parse_shader_sampler_combos(&source).map_err(|message| {
                    WeIngestError::InvalidProject(format!("{path}: {message}"))
                })?,
            );
        }
        Ok(combos)
    }
}
