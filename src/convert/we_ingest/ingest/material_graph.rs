use super::*;

impl WeIrBuilder {
    pub(super) fn add_material(&mut self, path: &str) -> Result<u32, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.material_by_path.get(&path) {
            return Ok(*handle);
        }
        let handle = self.materials.len() as u32;
        self.material_by_path.insert(path.clone(), handle);
        let resource = self.add_required_resource(&path, SceneResourceKind::MaterialJson)?;
        let payload = self.resources[resource as usize].payload.clone();
        let material_json = parse_json_bytes(&path, &payload)?;
        let pass_start = self.material_passes.len() as u32;
        for pass in material_json
            .get("passes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.add_material_pass(handle, &path, pass)?;
        }
        let pass_count = self.material_passes.len() as u32 - pass_start;
        if pass_count == 0 {
            self.unsupported.push(WeIrUnsupported {
                object: None,
                pass_index: None,
                feature: format!("material-has-no-passes:{path}"),
                expected_subsystem: "convert/we_ingest material parser".to_owned(),
                containment: "material-record-kept-without-passes".to_owned(),
            });
        }
        self.materials.push(WeIrMaterial {
            handle,
            resource,
            pass_start,
            pass_count,
        });
        Ok(handle)
    }

    pub(super) fn add_image_plane_mesh(
        &mut self,
        object: u32,
        material: Option<u32>,
        width: f32,
        height: f32,
    ) {
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let vertex_start = self.mesh_vertices.len() as u32;
        let index_start = self.mesh_indices.len() as u32;
        self.mesh_vertices.extend([
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -half_width,
                    y: -half_height,
                    z: 0.0,
                },
                uv: [0.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: half_width,
                    y: -half_height,
                    z: 0.0,
                },
                uv: [1.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: half_width,
                    y: half_height,
                    z: 0.0,
                },
                uv: [1.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -half_width,
                    y: half_height,
                    z: 0.0,
                },
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
        ]);
        self.mesh_indices.extend([0, 1, 2, 0, 2, 3]);
        self.meshes.push(WeIrMesh {
            object,
            material,
            vertex_start,
            vertex_count: 4,
            index_start,
            index_count: 6,
            width,
            height,
            bounds_min: SceneVec3 {
                x: -half_width,
                y: -half_height,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: half_width,
                y: half_height,
                z: 0.0,
            },
        });
    }

    pub(super) fn add_material_pass(
        &mut self,
        material: u32,
        material_path: &str,
        pass: &Value,
    ) -> Result<(), WeIngestError> {
        let texture_start = self.material_textures.len() as u32;
        for (slot, texture) in pass
            .get("textures")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(path) = bound_string(Some(texture)) {
                let resource = self.add_texture(&path, Some(material_path))?;
                self.material_textures.push(WeIrMaterialTexture {
                    slot: slot as u32,
                    resource,
                    path,
                });
            } else {
                self.material_textures.push(WeIrMaterialTexture {
                    slot: slot as u32,
                    resource: None,
                    path: String::new(),
                });
            }
        }
        let texture_count = self.material_textures.len() as u32 - texture_start;

        let constant_start = self.material_constants.len() as u32;
        if let Some(constants) = pass.get("constantshadervalues").and_then(Value::as_object) {
            for (name, value) in constants {
                self.material_constants.push(WeIrMaterialConstant {
                    name: name.clone(),
                    value_json: compact_json(value),
                });
            }
        }
        let constant_count = self.material_constants.len() as u32 - constant_start;

        self.material_passes.push(WeIrMaterialPass {
            material,
            shader_key: bound_string(pass.get("shader")).unwrap_or_default(),
            target: bound_string(pass.get("target")).unwrap_or_default(),
            texture_start,
            texture_count,
            constant_start,
            constant_count,
            pipeline_blend: pipeline_blend_from_we(pass.get("blending").and_then(Value::as_str)),
            depth_test: depth_test_from_we(pass.get("depthtest").and_then(Value::as_str)),
            depth_write: pass
                .get("depthwrite")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("enabled")),
            cull_mode: cull_mode_from_we(pass.get("cullmode").and_then(Value::as_str)),
            alpha_writing: pass
                .get("alphawriting")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            clear_target: pass.get("clear").and_then(Value::as_bool).unwrap_or(false),
        });
        Ok(())
    }

    pub(super) fn add_texture(
        &mut self,
        path: &str,
        material_path: Option<&str>,
    ) -> Result<Option<u32>, WeIngestError> {
        let original = normalize_we_path(path);
        if is_runtime_render_target(&original) {
            return Ok(None);
        }
        for candidate in texture_candidates(&original, material_path) {
            if let Some(resource) = self.texture_by_path.get(&candidate).copied() {
                return Ok(Some(resource));
            }
            let Some(asset) = self.source.read_optional_asset(&candidate)? else {
                continue;
            };
            let kind = if candidate.ends_with(".tex") {
                SceneResourceKind::TextureTex
            } else {
                SceneResourceKind::Raw
            };
            let resource = self.add_existing_resource(&candidate, kind, asset.source, asset.bytes);
            if candidate.ends_with(".tex") {
                match decode_tex_upload(&self.resources[resource as usize].payload).and_then(
                    |upload| {
                        let alpha_coverage_rows = texture_alpha_coverage_rows(&upload);
                        transcode_texture_upload(&candidate, upload)
                            .map(|upload| (upload, alpha_coverage_rows))
                    },
                ) {
                    Ok((upload, alpha_coverage_rows)) => self.textures.push(WeIrTexture {
                        resource,
                        format: upload.format,
                        source_runtime_format: upload.metadata.runtime_format,
                        payload_format: upload.metadata.payload_format,
                        sampler_flags: upload.metadata.sampler_flags,
                        width: upload.metadata.width,
                        height: upload.metadata.height,
                        storage_width: upload.metadata.storage_width,
                        storage_height: upload.metadata.storage_height,
                        texv_tag: upload.metadata.texv_tag,
                        texb_tag: upload.metadata.texb_tag,
                        mips: upload
                            .mips
                            .into_iter()
                            .map(|mip| WeIrTextureMip {
                                width: mip.width,
                                height: mip.height,
                                payload_offset: mip.payload_offset,
                                payload_len: mip.payload_len,
                            })
                            .collect(),
                        upload_payload: upload.payload,
                        alpha_coverage_rows,
                    }),
                    Err(source) => {
                        self.unsupported.push(WeIrUnsupported {
                            object: None,
                            pass_index: None,
                            feature: format!("tex-metadata-parse-failed:{candidate}:{source}"),
                            expected_subsystem: "convert/we_ingest tex parser".to_owned(),
                            containment: "texture-resource-kept-as-raw-payload".to_owned(),
                        });
                    }
                }
            }
            self.texture_by_path.insert(candidate.clone(), resource);
            return Ok(Some(resource));
        }
        self.unsupported.push(WeIrUnsupported {
            object: None,
            pass_index: None,
            feature: format!("missing-texture:{original}"),
            expected_subsystem: "convert/we_ingest texture resolver".to_owned(),
            containment: "texture-slot-kept-without-resource".to_owned(),
        });
        Ok(None)
    }

    pub(super) fn add_effect(&mut self, path: &str) -> Result<u32, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.effect_by_path.get(&path) {
            return Ok(*handle);
        }
        let handle = self.effects.len() as u32;
        self.effect_by_path.insert(path.clone(), handle);
        let resource = self.add_required_resource(&path, SceneResourceKind::EffectJson)?;
        let payload = self.resources[resource as usize].payload.clone();
        let effect_json = parse_json_bytes(&path, &payload)?;

        let fbo_start = self.effect_fbos.len() as u32;
        for fbo in effect_json
            .get("fbos")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = bound_string(fbo.get("name")).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let format =
                bound_string(fbo.get("format")).unwrap_or_else(|| "rgba_backbuffer".to_owned());
            let scale = value_f32(fbo.get("scale")).unwrap_or(1.0);
            self.effect_fbos.push(WeIrEffectFbo {
                name: name.clone(),
                format: format.clone(),
                scale,
            });
            self.image_targets.push(WeIrImageTarget {
                role: image_target_role(&name),
                name,
                format,
                width_divisor_milli: scale_divisor_to_milli(scale),
                height_divisor_milli: scale_divisor_to_milli(scale),
            });
        }
        let fbo_count = self.effect_fbos.len() as u32 - fbo_start;

        let pass_start = self.effect_passes.len() as u32;
        for (pass_index, pass) in effect_json
            .get("passes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            self.add_effect_pass(handle, pass_index as u32, pass)?;
        }
        let pass_count = self.effect_passes.len() as u32 - pass_start;

        self.effects.push(WeIrEffect {
            handle,
            resource,
            replacement_key: bound_string(effect_json.get("replacementkey")).unwrap_or_default(),
            pass_start,
            pass_count,
            fbo_start,
            fbo_count,
        });
        Ok(handle)
    }

    pub(super) fn add_effect_pass(
        &mut self,
        effect: u32,
        pass_index: u32,
        pass: &Value,
    ) -> Result<(), WeIngestError> {
        let binding_start = self.effect_bindings.len() as u32;
        self.push_effect_pass_bindings(pass);
        let binding_count = self.effect_bindings.len() as u32 - binding_start;

        let combo_start = self.effect_combos.len() as u32;
        if let Some(combos) = pass.get("combos").and_then(Value::as_object) {
            for (name, value) in combos {
                if let Some(value) = value_i64(Some(value)) {
                    self.effect_combos.push(WeIrEffectCombo {
                        name: name.clone(),
                        value,
                    });
                }
            }
        }
        let combo_count = self.effect_combos.len() as u32 - combo_start;

        let material = bound_string(pass.get("material"))
            .map(|path| self.add_material(&path))
            .transpose()?;
        self.effect_passes.push(WeIrEffectPass {
            effect,
            pass_index,
            material,
            command: bound_string(pass.get("command")).unwrap_or_default(),
            source: bound_string(pass.get("source")).unwrap_or_default(),
            target: bound_string(pass.get("target")).unwrap_or_default(),
            binding_start,
            binding_count,
            combo_start,
            combo_count,
        });
        Ok(())
    }

    pub(super) fn push_effect_pass_bindings(&mut self, pass: &Value) {
        if let Some(bindings) = pass.get("bind").and_then(Value::as_array) {
            for binding in bindings {
                let slot = value_u32(binding.get("index"))
                    .or_else(|| value_u32(binding.get("slot")))
                    .unwrap_or(0);
                let target = bound_string(binding.get("target"))
                    .or_else(|| bound_string(binding.get("source")))
                    .or_else(|| bound_string(binding.get("name")))
                    .unwrap_or_default();
                if !target.is_empty() {
                    self.effect_bindings
                        .push(WeIrEffectBinding { slot, target });
                }
            }
        }
        if let Some(textures) = pass.get("textures").and_then(Value::as_array) {
            for (slot, texture) in textures.iter().enumerate() {
                if let Some(path) = bound_string(Some(texture)) {
                    self.effect_bindings.push(WeIrEffectBinding {
                        slot: slot as u32,
                        target: path,
                    });
                } else if slot == 0 {
                    self.effect_bindings.push(WeIrEffectBinding {
                        slot: slot as u32,
                        target: "previous".to_owned(),
                    });
                }
            }
        }
    }

    pub(super) fn add_render_graph_for_object(
        &mut self,
        object: u32,
        material: u32,
        effect_instances: &[(u32, Value)],
        color_blend_mode: i32,
        utility_layer: Option<WeIrUtilityLayerKind>,
        object_is_puppet: bool,
    ) -> Result<u32, WeIngestError> {
        let graph_index = self.render_graphs.len() as u32;
        let base_material_handle = material;
        let material = &self.materials[base_material_handle as usize];
        let base_pass = self
            .material_passes
            .get(material.pass_start as usize)
            .cloned();
        let base_texture_slots = base_pass
            .as_ref()
            .map(|pass| {
                self.material_textures
                    .iter()
                    .skip(pass.texture_start as usize)
                    .take(pass.texture_count as usize)
                    .map(|texture| texture.slot)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let base_pass_constants =
            material_pass_constant_names(&self.material_constants, base_pass.as_ref());
        let effects_in_authored_texture_space = puppet_material::image_effects_use_authored_texture(
            base_pass.as_ref().map_or("", |pass| &pass.shader_key),
        );
        let mut effect_passes = Vec::new();
        for (effect_handle, instance) in effect_instances {
            self.push_effect_contracts_for_instance(
                object,
                *effect_handle,
                instance,
                &mut effect_passes,
            )?;
        }
        let final_scene_blend = scene_blend_from_color_blend_mode(color_blend_mode);
        let waterwaves_displacement =
            waterwaves_displacement::create_waterwaves_displacement_materials(
                self,
                effects_in_authored_texture_space,
                base_material_handle as usize,
                final_scene_blend,
                object_is_puppet,
                &effect_passes,
            );
        let foliage_ripple = foliage_ripple::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
        );
        let ripple_flow_materials = ripple_flow::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
        );
        let final_effect = final_effect::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
            effects_in_authored_texture_space,
            object_is_puppet,
        );
        let mut graph_contract = WeImageGraphContract {
            object_index: object as usize,
            base_material_index: Some(base_material_handle as usize),
            base_shader: base_pass.as_ref().and_then(|pass| {
                if pass.shader_key.is_empty() {
                    None
                } else {
                    Some(pass.shader_key.clone())
                }
            }),
            base_material_blending: base_pass
                .as_ref()
                .map(|pass| pipeline_blend_string(pass.pipeline_blend)),
            base_texture_slots,
            base_pass_constants,
            framebuffer_snapshot: utility_layer
                .filter(|layer| layer.samples_scene_color())
                .map(
                    |layer| crate::engine::render_graph::WeFramebufferSnapshotContract {
                        target_name: FULL_FRAMEBUFFER_TARGET.to_owned(),
                        texture_slot: 0,
                        composite_to_object_mesh: matches!(
                            layer,
                            WeIrUtilityLayerKind::FramebufferComposite
                        ),
                    },
                ),
            final_scene_blend,
            effects_in_authored_texture_space,
            puppet_skinning_after_effects: object_is_puppet && effects_in_authored_texture_space,
            waterwaves_uv_field_material_index: waterwaves_displacement.uv_field,
            waterwaves_direct_material: waterwaves_displacement.direct,
            foliage_ripple_material: foliage_ripple,
            ripple_flow_material_indices: ripple_flow_materials,
            final_effect_material: final_effect,
            effect_passes,
        };
        if graph_contract.framebuffer_snapshot.is_none()
            && we_image_graph_requires_generated_scene_snapshot(&graph_contract)
        {
            graph_contract.framebuffer_snapshot =
                Some(crate::engine::render_graph::WeFramebufferSnapshotContract {
                    target_name: FULL_FRAMEBUFFER_TARGET.to_owned(),
                    texture_slot: 0,
                    composite_to_object_mesh: false,
                });
        }
        let has_framebuffer_snapshot = graph_contract.framebuffer_snapshot.is_some();
        let mut graph = we_image_graph(&graph_contract);
        puppet_clipping::apply_token_one_graph(self, object, base_material_handle, &mut graph);
        if has_framebuffer_snapshot
            && !self.image_targets.iter().any(|target| {
                target.name == FULL_FRAMEBUFFER_TARGET
                    && target.role == WeIrImageTargetRole::FirstClassEffectTarget
            })
        {
            self.image_targets.push(WeIrImageTarget {
                name: FULL_FRAMEBUFFER_TARGET.to_owned(),
                format: "rgba_backbuffer".to_owned(),
                role: WeIrImageTargetRole::FirstClassEffectTarget,
                width_divisor_milli: 1_000,
                height_divisor_milli: 1_000,
            });
        }
        self.render_graphs.push(graph);
        Ok(graph_index)
    }

    pub(super) fn push_effect_contracts_for_instance(
        &mut self,
        object: u32,
        effect_handle: u32,
        instance: &Value,
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
            apply_builtin_effect_texture_defaults(&effect_file, &combos, &mut binds);
            let base_shader = material_pass
                .as_ref()
                .map(|pass| pass.shader_key.clone())
                .filter(|shader| !shader.is_empty());
            let combo_defaults = base_shader
                .as_deref()
                .map(|shader| self.shader_combo_defaults(shader))
                .transpose()?
                .unwrap_or_default();
            let shader = base_shader
                .as_deref()
                .map(|shader| effect_shader_variant_key(shader, &binds, &combos, &combo_defaults));
            let (material_index, pass_constants) =
                match (base_material, material_pass.clone(), shader.as_deref()) {
                    (Some(material), Some(pass), Some(shader)) => {
                        let material = self.add_effect_material_instance(
                            material,
                            pass,
                            base_textures,
                            instance_pass,
                            &binds,
                            shader,
                        )?;
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
                material_index,
                effect_file: effect_file.clone(),
                pass_index: local_index,
                command: non_empty_string(&effect_pass.command),
                shader,
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

    pub(super) fn add_effect_material_instance(
        &mut self,
        base_material: WeIrMaterial,
        base_pass: WeIrMaterialPass,
        base_textures: Vec<WeIrMaterialTexture>,
        instance_pass: Option<&Value>,
        resolved_bindings: &BTreeMap<u32, String>,
        shader_key: &str,
    ) -> Result<u32, WeIngestError> {
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
        let handle = self.materials.len() as u32;
        let texture_start = self.material_textures.len() as u32;
        self.material_textures.extend(textures.into_values());
        let constant_start = self.material_constants.len() as u32;
        self.material_constants.extend(constants);
        let mut pass = base_pass;
        pass.material = handle;
        pass.shader_key = shader_key.to_owned();
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
        Ok(handle)
    }

    pub(super) fn shader_combo_defaults(
        &mut self,
        shader_key: &str,
    ) -> Result<BTreeMap<String, i64>, WeIngestError> {
        let shader_key = shader_key.split("__").next().unwrap_or(shader_key);
        if let Some(defaults) = self.shader_combo_defaults_by_shader.get(shader_key) {
            return Ok(defaults.clone());
        }
        let mut defaults = BTreeMap::new();
        for extension in ["vert", "frag"] {
            let path = format!("shaders/{shader_key}.{extension}");
            let Some(asset) = self.source.read_optional_asset(&path)? else {
                continue;
            };
            let source = String::from_utf8_lossy(&asset.bytes);
            for definition in parse_shader_combo_definitions(shader_key, &source) {
                defaults
                    .entry(definition.name.clone())
                    .or_insert(definition.default_value);
                if !self.shader_combo_definitions.iter().any(|existing| {
                    existing
                        .shader_key
                        .eq_ignore_ascii_case(&definition.shader_key)
                        && existing.name.eq_ignore_ascii_case(&definition.name)
                }) {
                    self.shader_combo_definitions.push(definition);
                }
            }
        }
        self.shader_combo_defaults_by_shader
            .insert(shader_key.to_owned(), defaults.clone());
        Ok(defaults)
    }

    pub(super) fn build_shader_contracts(&mut self) {
        self.shader_contracts = build_shader_contract_records(
            &self.render_graphs,
            &self.material_passes,
            &self.material_textures,
            &self.material_constants,
        );
    }
}
