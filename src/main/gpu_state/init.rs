impl WgpuState {
    fn new(window: Arc<WaylandWindow>) -> Result<Self, String> {
        let size = nonzero_size(window.surface_size());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });

        let surface = instance
            .create_surface(window.surface_handle())
            .map_err(|error| format!("create surface: {error}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("request adapter: {error}"))?;

        let adapter_info = adapter.get_info();
        fika_log!(
            "[fika-wgpu] adapter name={:?} backend={:?}",
            adapter_info.name,
            adapter_info.backend
        );

        // Enable Vulkan dmabuf import when the driver supports it (wgpu-hal
        // texture_from_dmabuf_fd). Present still uses RWH/swapchain; this only
        // unlocks sampling external dmabuf textures.
        let dmabuf_features = crate::shell::render::dmabuf::optional_dmabuf_features(&adapter);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fika-wgpu-device"),
            required_features: dmabuf_features,
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("request device: {error}"))?;

        let dmabuf_import_supported =
            dmabuf_features.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF);
        if dmabuf_import_supported {
            fika_log!("[fika-wgpu] dmabuf-import enabled (VULKAN_EXTERNAL_MEMORY_DMA_BUF)");
        } else {
            fika_log!("[fika-wgpu] dmabuf-import unavailable on this adapter");
        }

        Self::from_surface_parts(size, instance, adapter, device, queue, surface)
    }

    fn new_with_shared_device(window: Arc<WaylandWindow>, shared: &Self) -> Result<Self, String> {
        let size = nonzero_size(window.surface_size());
        let instance = shared.instance.clone();
        let adapter = shared.adapter.clone();
        let device = shared.device.clone();
        let queue = shared.queue.clone();
        let surface = instance
            .create_surface(window.surface_handle())
            .map_err(|error| format!("create surface: {error}"))?;
        fika_dialog_trace!(
            "[fika-wgpu] renderer-shared-device adapter={:?}",
            adapter.get_info().name
        );
        Self::from_surface_parts(size, instance, adapter, device, queue, surface)
    }

    fn from_surface_parts(
        size: PhysicalSize<u32>,
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
    ) -> Result<Self, String> {
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "surface has no supported formats".to_string())?;
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .unwrap_or_else(|| capabilities.present_modes[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::PreMultiplied)
            .or_else(|| {
                capabilities
                    .alpha_modes
                    .iter()
                    .copied()
                    .find(|mode| *mode == wgpu::CompositeAlphaMode::PostMultiplied)
            })
            .or_else(|| capabilities.alpha_modes.first().copied())
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        fika_log!(
            "[fika-wgpu] surface-format={format:?} srgb={} alpha={alpha_mode:?}",
            format.is_srgb() as u8,
        );

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width,
            height: size.height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let damage_clear_renderer =
            QuadRenderer::new_transparent_clear(&device, &queue, config.format);
        let quad_renderer = QuadRenderer::new(&device, config.format);
        let overlay_quad_renderer = QuadRenderer::new(&device, config.format);
        let icon_renderer = IconRenderer::new(&device, config.format);
        let text_renderer = TextRenderer::new(&device, config.format);
        let retained_scene = RetainedSceneRenderer::new(
            &device,
            &queue,
            config.format,
            size,
        );

        let dmabuf_import_supported =
            crate::shell::render::dmabuf::adapter_supports_dmabuf_import(&adapter)
                && device
                    .features()
                    .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF);

        let mut icon_renderer = icon_renderer;
        icon_renderer.set_dmabuf_import_state(dmabuf_import_supported, None);

        Ok(Self {
            damage_clear_renderer,
            quad_renderer,
            overlay_quad_renderer,
            icon_renderer,
            text_renderer,
            overlay_text_renderer: None,
            retained_scene,
            surface,
            queue,
            device,
            adapter,
            instance,
            config,
            size,
            frame_count: 0,
            last_log: Instant::now(),
            rendered_view_switches: 0,
            last_render_dirty_key: None,
            last_render_damage_snapshot: None,
            frame_latency: ShellFrameLatencyTracker::default(),
            render_work_pending: false,
            clean_redraw_skips: 0,
            dmabuf_import_supported,
        })
    }

    /// Push negotiated dmabuf plan into the icon renderer (scheme C import path).
    pub(crate) fn sync_icon_dmabuf_plan(
        &mut self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
    ) {
        let readiness = self.dmabuf_readiness(event_loop, surface);
        self.icon_renderer.set_dmabuf_import_state(
            readiness.vulkan_import,
            readiness.plan,
        );
    }

    /// Whether this renderer can wrap dmabuf fds as wgpu textures.
    #[allow(dead_code)]
    pub(crate) fn dmabuf_import_supported(&self) -> bool {
        self.dmabuf_import_supported
    }

    /// Protocol + GPU readiness for external dmabuf import.
    pub(crate) fn dmabuf_readiness(
        &self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
    ) -> crate::shell::render::dmabuf::DmabufReadiness {
        let wayland_global = event_loop.has_linux_dmabuf();
        let feedback = match surface {
            Some(id) => event_loop.dmabuf_feedback_for(id),
            None => event_loop.dmabuf_default_feedback(),
        };
        crate::shell::render::dmabuf::assess_readiness(
            self.dmabuf_import_supported,
            wayland_global,
            feedback.as_ref(),
        )
    }

    /// Negotiated import plan (format/modifier) for a surface, if ready.
    #[allow(dead_code)]
    pub(crate) fn dmabuf_import_plan(
        &self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
    ) -> Option<crate::shell::render::dmabuf::DmabufImportPlan> {
        self.dmabuf_readiness(event_loop, surface).plan
    }

    /// Import a single-plane dmabuf as a sampleable wgpu texture.
    ///
    /// Present still uses the RWH swapchain; use this for external content
    /// (thumbnails, video frames) that should be drawn into the retained scene.
    #[allow(dead_code)]
    pub(crate) fn import_dmabuf_texture(
        &self,
        desc: crate::shell::render::dmabuf::DmabufImportDesc,
    ) -> Result<wgpu::Texture, crate::shell::render::dmabuf::DmabufImportError> {
        if !self.dmabuf_import_supported {
            return Err(crate::shell::render::dmabuf::DmabufImportError::FeatureUnavailable);
        }
        crate::shell::render::dmabuf::import_dmabuf_texture(&self.device, desc)
    }

    /// Import using a negotiated plan (fills fourcc from the plan).
    #[allow(dead_code)]
    pub(crate) fn import_dmabuf_with_plan(
        &self,
        plan: crate::shell::render::dmabuf::DmabufImportPlan,
        width: u32,
        height: u32,
        plane: crate::shell::render::dmabuf::DmabufImportPlane,
        usage: wgpu::TextureUsages,
        label: Option<&'static str>,
    ) -> Result<wgpu::Texture, crate::shell::render::dmabuf::DmabufImportError> {
        let desc = crate::shell::render::dmabuf::import_desc_from_plan(
            plan, width, height, plane, usage, label,
        );
        self.import_dmabuf_texture(desc)
    }

    /// Acquire external content: dmabuf import when possible, else CPU upload.
    ///
    /// Icon/text atlases still use their own `write_texture` paths. Use this for
    /// whole-texture external content (video frames, future GPU producers).
    #[allow(dead_code)]
    pub(crate) fn acquire_external_texture(
        &self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
        width: u32,
        height: u32,
        plane: Option<crate::shell::render::dmabuf::DmabufImportPlane>,
        pixels: Option<&[u8]>,
        label: Option<&'static str>,
    ) -> Result<
        crate::shell::render::dmabuf::ExternalTexture,
        crate::shell::render::dmabuf::DmabufImportError,
    > {
        let plan = self.dmabuf_import_plan(event_loop, surface);
        crate::shell::render::dmabuf::acquire_external_texture(
            &self.device,
            &self.queue,
            plan,
            width,
            height,
            plane,
            pixels,
            label,
        )
    }

    /// Log one-line readiness after feedback may have arrived.
    pub(crate) fn log_dmabuf_readiness(
        &self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
        reason: &'static str,
    ) {
        let r = self.dmabuf_readiness(event_loop, surface);
        fika_log!(
            "[fika-wgpu] dmabuf-status reason={reason} import_ready={} {}",
            r.import_ready() as u8,
            r.summary()
        );
    }

    pub(crate) fn wait_idle(&self, reason: &'static str) {
        let start = Instant::now();
        match self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(500)),
        }) {
            Ok(status) => {
                fika_dialog_trace!(
                    "[fika-wgpu] renderer-idle reason={} status={:?} elapsed_ms={}",
                    reason,
                    status,
                    start.elapsed().as_millis()
                );
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] renderer-idle-failed reason={} error={:?}",
                    reason,
                    error
                );
            }
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.configure_surface(size, false);
    }

    fn force_reconfigure(&mut self, size: PhysicalSize<u32>) {
        self.configure_surface(size, true);
    }

    fn configure_surface(&mut self, size: PhysicalSize<u32>, force: bool) {
        let size = nonzero_size(size);
        if self.size == size && !force {
            return;
        }

        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.retained_scene.resize(&self.device, size);
        fika_log!(
            "[fika-wgpu] {} width={} height={}",
            if force { "reconfigure" } else { "resize" },
            size.width,
            size.height
        );
    }

    fn acquire_surface_frame(
        &mut self,
        window: &WaylandWindow,
        reason: &'static str,
        context: ShellSurfaceFrameContext,
    ) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                if context.reconfigure_on_suboptimal() {
                    // `configure` and the retry below both require the currently acquired
                    // surface texture to have been released first.
                    drop(frame);
                    context.log_retry(reason);
                    self.force_reconfigure(window.surface_size());
                    self.acquire_surface_frame_after_reconfigure(window, reason, context)
                } else {
                    Some(frame)
                }
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                context.log_retry(reason);
                self.force_reconfigure(window.surface_size());
                self.acquire_surface_frame_after_reconfigure(window, reason, context)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                context.log_not_ready(reason);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                context.log_validation();
                None
            }
        }
    }

    fn acquire_surface_frame_after_reconfigure(
        &mut self,
        window: &WaylandWindow,
        reason: &'static str,
        context: ShellSurfaceFrameContext,
    ) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                context.log_reconfigure_pending(reason);
                window.request_redraw();
                None
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                context.log_not_ready(reason);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                context.log_validation();
                None
            }
        }
    }

    fn submit_surface_frame(
        &mut self,
        window: &WaylandWindow,
        frame: wgpu::SurfaceTexture,
        encoder: wgpu::CommandEncoder,
    ) -> u64 {
        self.queue.submit(Some(encoder.finish()));
        window.pre_present_notify();
        self.queue.present(frame);
        self.frame_count += 1;
        self.frame_count
    }

    fn begin_surface_frame_encoding(
        &self,
        frame: &wgpu::SurfaceTexture,
        label: &'static str,
    ) -> (wgpu::TextureView, wgpu::CommandEncoder) {
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        (view, encoder)
    }

    fn encode_detached_dialog_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        popup_theme: PopupTheme,
    ) {
        let [r, g, b, a] = popup_theme.surface;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fika-wgpu-detached-dialog-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: r as f64,
                        g: g as f64,
                        b: b as f64,
                        a: a as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.quad_renderer.draw(&mut pass);
        self.icon_renderer.draw(&mut pass);
        self.text_renderer.draw(&mut pass);
        self.icon_renderer.draw_overlay(&mut pass);
    }

    fn encode_retained_scene_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        damage: ShellRenderDamage,
        scissor: Option<DamageScissorRect>,
        overlay_text_active: bool,
    ) {
        if damage.kind == ShellRenderDamageKind::Clean {
            return;
        }

        let load = if damage.kind == ShellRenderDamageKind::Full {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fika-wgpu-retained-scene-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.retained_scene.view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if damage.kind == ShellRenderDamageKind::Bounded
            && let Some(scissor) = scissor
        {
            pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
            self.damage_clear_renderer.draw(&mut pass);
        }
        self.quad_renderer.draw(&mut pass);
        self.icon_renderer.draw(&mut pass);
        self.text_renderer.draw(&mut pass);
        self.overlay_quad_renderer.draw(&mut pass);
        self.icon_renderer.draw_overlay(&mut pass);
        if overlay_text_active
            && let Some(overlay_text_renderer) = self.overlay_text_renderer.as_ref()
        {
            overlay_text_renderer.draw(&mut pass);
        }
    }

    fn encode_retained_present_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fika-wgpu-retained-present-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.retained_scene.draw(&mut pass);
    }

    fn prewarm_scene_caches(&mut self, scene: &mut ShellScene, reason: &'static str) {
        let total_start = Instant::now();
        let metadata_result_stats = scene.drain_metadata_role_results();
        let role_start = Instant::now();
        let mut projection_layouts = scene.prepare_frame_projection_layouts(self.size);
        scene.update_visible_slot_pools_for_projection_layouts(&mut projection_layouts);
        let frame_projections = scene.pane_projections_from_layouts(projection_layouts);
        let _folder_preview_role_stats =
            scene.update_folder_preview_roles_for_projections(frame_projections.projections());
        let folder_preview_results = scene.drain_folder_preview_role_results();
        let metadata_role_stats =
            scene.prewarm_file_metadata_roles(frame_projections.projections());
        for projection in frame_projections.projections() {
            if let Some(item) = projection.visible_items.first() {
                let icon_size = item
                    .layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0);
                self.icon_renderer
                    .prewarm_common_file_icon_rasters(icon_size);
            }
        }
        let role_stats = scene.prewarm_visible_file_icon_roles(
            frame_projections.projections(),
            &mut self.icon_renderer.resolver,
            reason,
        );
        let scene_icon_raster_prewarm_stats = if visible_exact_icon_roles_enabled_for_frame(reason)
        {
            self.icon_renderer
                .prewarm_small_directory_file_icon_rasters(frame_projections.projections())
        } else {
            IconRasterPrewarmStats::default()
        };
        let scene_text_prewarm_stats = self.prewarm_text_labels(
            scene,
            frame_projections.projections(),
            text_label_prewarm_mode_for_scene_prewarm(reason),
        );
        self.frame_latency.observe_async_results(
            ShellFrameLatencyAsyncResults {
                metadata_applied: metadata_result_stats.applied as u64,
                folder_preview_results: folder_preview_results.applied as u64,
                ..ShellFrameLatencyAsyncResults::default()
            },
            self.frame_count,
        );
        let role_prewarm_us = role_start.elapsed().as_micros();
        if scene_icon_raster_prewarm_stats.entries > 0
            && (scene_icon_raster_prewarm_stats.raster_us >= 1000
                || scene_icon_raster_prewarm_stats.over_budget
                || fika_frame_log_all_enabled())
        {
            fika_log!(
                "[fika-wgpu] prewarm-icon-rasters-scene reason={} view={} entries={} hits={} misses={} failed={} raster={}us over_budget={}",
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                scene_icon_raster_prewarm_stats.entries,
                scene_icon_raster_prewarm_stats.cache_hits,
                scene_icon_raster_prewarm_stats.cache_misses,
                scene_icon_raster_prewarm_stats.failed,
                scene_icon_raster_prewarm_stats.raster_us,
                scene_icon_raster_prewarm_stats.over_budget as u8
            );
        }
        if scene_text_prewarm_stats.entries + scene_text_prewarm_stats.read_ahead > 0
            && (scene_text_prewarm_stats.raster_us >= 1000
                || scene_text_prewarm_stats.over_budget
                || fika_frame_log_all_enabled())
        {
            fika_log!(
                "[fika-wgpu] prewarm-text-scene reason={} view={} entries={} read_ahead={} hits={} misses={} deferred={} raster={}us over_budget={}",
                reason,
                scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
                scene_text_prewarm_stats.entries,
                scene_text_prewarm_stats.read_ahead,
                scene_text_prewarm_stats.cache_hits,
                scene_text_prewarm_stats.cache_misses,
                scene_text_prewarm_stats.deferred,
                scene_text_prewarm_stats.raster_us,
                scene_text_prewarm_stats.over_budget as u8
            );
        }
        let prepare_start = Instant::now();
        let overlay_text_active = scene.overlay_text_needed();
        if overlay_text_active && self.overlay_text_renderer.is_none() {
            fika_log!("[fika-wgpu] overlay-text init reason={reason}");
            self.overlay_text_renderer = Some(TextRenderer::new(&self.device, self.config.format));
        }
        let scene_frame = prepare_scene_frame(
            SceneFrameRenderers {
                text: &mut self.text_renderer,
                overlay_text: if overlay_text_active {
                    self.overlay_text_renderer.as_mut()
                } else {
                    None
                },
                icons: &mut self.icon_renderer,
            },
            FrameGpuContext {
                device: &self.device,
                queue: &self.queue,
            },
            SceneFrameRequest {
                scene,
                projections: &frame_projections,
                size: self.size,
                reason,
            },
        );
        let prepare_us = prepare_start.elapsed().as_micros();
        fika_log!(
            "[fika-wgpu] prewarm-scene reason={} view={} visible={} metadata_visible={} metadata_deferred={} metadata_batches={} metadata_results={} metadata_applied={} role_entries={} role_deferred={} role_read_ahead={} role_resolve={}us role_total={}us text_labels={} text_cache={}/{} text_deferred={} text_raster={}us icon_resolve={}us icon_raster={}us icon_deferred={} icon_raster_deferred={} prepare={}us total={}us",
            reason,
            scene.panes[ShellPaneId::SLOT_0].view_mode.as_str(),
            scene_frame.visible_items,
            metadata_role_stats.visible,
            metadata_role_stats.deferred,
            metadata_role_stats.batches_started,
            metadata_result_stats.results,
            metadata_result_stats.applied,
            role_stats.entries,
            role_stats.deferred,
            role_stats.read_ahead,
            role_stats.resolve_us,
            role_prewarm_us,
            scene_frame.text_stats.labels,
            scene_frame.text_stats.cache_hits,
            scene_frame.text_stats.cache_misses,
            scene_frame.text_stats.deferred,
            scene_frame.text_stats.raster_us,
            scene_frame.icon_stats.resolve_us,
            scene_frame.icon_stats.raster_us,
            scene_frame.icon_stats.deferred,
            scene_frame.icon_stats.raster_deferred,
            prepare_us,
            total_start.elapsed().as_micros()
        );
    }
}
