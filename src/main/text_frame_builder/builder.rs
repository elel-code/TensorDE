struct TextFrameResources<'a> {
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    text_buffer: &'a mut Buffer,
    label_cache: &'a mut LabelRasterCache,
    metrics_cache: &'a mut LabelMetricsCache,
    atlas_cache: &'a mut TextAtlasFrameCache,
}

impl<'a> TextFrameResources<'a> {
    fn new(
        font_system: &'a mut FontSystem,
        swash_cache: &'a mut SwashCache,
        text_buffer: &'a mut Buffer,
        label_cache: &'a mut LabelRasterCache,
        metrics_cache: &'a mut LabelMetricsCache,
        atlas_cache: &'a mut TextAtlasFrameCache,
    ) -> Self {
        Self {
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            metrics_cache,
            atlas_cache,
        }
    }

    fn from_renderer(renderer: &'a mut TextRenderer) -> Self {
        Self::new(
            &mut renderer.font_system,
            &mut renderer.swash_cache,
            &mut renderer.text_buffer,
            &mut renderer.label_cache,
            &mut renderer.metrics_cache,
            &mut renderer.atlas_cache,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct TextLabelLayout {
    draw: ViewRect,
    layout: ViewRect,
    clip: ViewRect,
}

#[derive(Clone, Copy, Debug)]
struct TextLabelStyle {
    color: TextColor,
    alignment: LabelAlignment,
    wrap: LabelWrap,
}

impl<'a> TextFrameBuilder<'a> {
    fn new(
        resources: TextFrameResources<'a>,
        surface_size: PhysicalSize<u32>,
        text_scale_factor: f32,
        atlas_pixels: Vec<u8>,
    ) -> Self {
        let TextFrameResources {
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            metrics_cache,
            atlas_cache,
        } = resources;
        let atlas_width = atlas_cache.width;
        let max_line_height = (TEXT_LINE_HEIGHT * text_scale_factor).round().max(1.0);
        let max_font_size = (TEXT_FONT_SIZE * max_line_height / TEXT_LINE_HEIGHT).max(1.0);
        let text_midline_shift =
            dolphin_text_midline_shift_for_font(font_system, max_font_size, max_line_height);
        text_buffer.set_metrics(Metrics::new(max_font_size, max_line_height));
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        Self {
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            metrics_cache,
            atlas_cache,
            surface_size,
            max_font_size,
            max_line_height,
            pending_draws: Vec::with_capacity(64),
            width: atlas_width,
            labels: 0,
            cache_hits: 0,
            cache_misses: 0,
            deferred: 0,
            raster_miss_budget: default_text_raster_miss_budget(),
            raster_us: 0,
            atlas_pixels,
            text_midline_shift,
        }
    }

    fn dolphin_midline_shift(&self) -> f32 {
        self.text_midline_shift
    }

    fn push_label(&mut self, label: &str, rect: ViewRect, clip: ViewRect, color: TextColor) {
        self.push_label_aligned(label, rect, clip, color, LabelAlignment::Center);
    }

    fn push_label_aligned(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) {
        self.push_label_aligned_wrapped(
            label,
            rect,
            clip,
            color,
            alignment,
            LabelWrap::WordOrGlyph,
        );
    }

    fn push_label_aligned_no_wrap(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) {
        self.push_label_aligned_wrapped(label, rect, clip, color, alignment, LabelWrap::None);
    }

    fn push_filename_label_aligned_no_wrap_with_layout(
        &mut self,
        label: &str,
        draw_rect: ViewRect,
        layout_rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) {
        let display = dolphin_elide_filename_to_width_shaped(
            self.font_system,
            self.text_buffer,
            label,
            layout_rect.width,
            self.max_font_size,
            self.max_line_height,
        );
        self.push_label_aligned_wrapped_with_layout(
            &display,
            TextLabelLayout {
                draw: draw_rect,
                layout: layout_rect,
                clip,
            },
            TextLabelStyle {
                color,
                alignment,
                wrap: LabelWrap::None,
            },
        );
    }

    fn push_filename_label_wrapped_with_layout(
        &mut self,
        label: &str,
        draw_rect: ViewRect,
        layout_rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
    ) {
        let display = dolphin_layout_icons_filename(
            self.font_system,
            self.text_buffer,
            label,
            layout_rect.width,
            DOLPHIN_ICONS_MAX_TEXT_LINES,
            self.max_font_size,
            self.max_line_height,
        )
        .display;
        self.push_label_aligned_wrapped_with_layout(
            &display,
            TextLabelLayout {
                draw: draw_rect,
                layout: layout_rect,
                clip,
            },
            TextLabelStyle {
                color,
                alignment: LabelAlignment::Center,
                wrap: LabelWrap::WordOrGlyph,
            },
        );
    }

    fn push_label_aligned_wrapped(
        &mut self,
        label: &str,
        rect: ViewRect,
        clip: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) {
        self.push_label_aligned_wrapped_with_layout(
            label,
            TextLabelLayout {
                draw: rect,
                layout: rect,
                clip,
            },
            TextLabelStyle {
                color,
                alignment,
                wrap,
            },
        );
    }

    fn push_label_aligned_wrapped_with_layout(
        &mut self,
        label: &str,
        layout: TextLabelLayout,
        style: TextLabelStyle,
    ) {
        let TextLabelLayout {
            draw: draw_rect,
            layout: layout_rect,
            clip,
        } = layout;
        let TextLabelStyle {
            color,
            alignment,
            wrap,
        } = style;
        let Some((key, adjusted_layout_rect, label_width, label_height)) =
            self.label_raster_key(label, layout_rect, alignment, wrap)
        else {
            return;
        };
        let rect = map_layout_rect_to_draw_rect(layout_rect, draw_rect, adjusted_layout_rect);
        let Some(screen) = intersect_rect(rect, clip) else {
            return;
        };

        let Some((label_pixels, outcome)) =
            self.resolve_label_pixels(label, &key, label_width, label_height, alignment, wrap)
        else {
            return;
        };

        self.pending_draws.push(PendingTextDraw {
            key,
            pixels: label_pixels,
            atlas_upload_required: outcome == LabelCacheOutcome::Miss,
            screen,
            rect,
            label_width,
            label_height,
            color,
        });
        self.labels += 1;
    }

    fn set_raster_miss_budget(&mut self, budget: usize) {
        self.raster_miss_budget = budget;
    }

    fn prewarm_filename_label_aligned_no_wrap(
        &mut self,
        label: &str,
        rect: ViewRect,
        color: TextColor,
        alignment: LabelAlignment,
    ) -> LabelCacheOutcome {
        let display = dolphin_elide_filename_to_width_shaped(
            self.font_system,
            self.text_buffer,
            label,
            rect.width,
            self.max_font_size,
            self.max_line_height,
        );
        self.prewarm_label_aligned_wrapped(&display, rect, color, alignment, LabelWrap::None)
    }

    fn prewarm_filename_label_wrapped(
        &mut self,
        label: &str,
        rect: ViewRect,
        color: TextColor,
    ) -> LabelCacheOutcome {
        let display = dolphin_layout_icons_filename(
            self.font_system,
            self.text_buffer,
            label,
            rect.width,
            DOLPHIN_ICONS_MAX_TEXT_LINES,
            self.max_font_size,
            self.max_line_height,
        )
        .display;
        self.prewarm_label_aligned_wrapped(
            &display,
            rect,
            color,
            LabelAlignment::Center,
            LabelWrap::WordOrGlyph,
        )
    }

    fn prewarm_label_aligned_wrapped(
        &mut self,
        label: &str,
        rect: ViewRect,
        _color: TextColor,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> LabelCacheOutcome {
        let Some((key, _, label_width, label_height)) =
            self.label_raster_key(label, rect, alignment, wrap)
        else {
            return LabelCacheOutcome::Skipped;
        };
        self.resolve_label_pixels(label, &key, label_width, label_height, alignment, wrap)
            .map(|(_, outcome)| outcome)
            .unwrap_or(LabelCacheOutcome::Deferred)
    }

    fn label_raster_key(
        &mut self,
        label: &str,
        mut rect: ViewRect,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> Option<(LabelCacheKey, ViewRect, u32, u32)> {
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return None;
        }
        let max_label_width = text_atlas_max_label_width(self.width);
        let label_height = rect.height.ceil().max(1.0) as u32;
        let label_width = if alignment == LabelAlignment::Start && wrap == LabelWrap::None {
            let natural_width =
                self.cached_no_wrap_label_width(label, label_height, max_label_width);
            let width = natural_width
                .min(rect.width.ceil().max(1.0) as u32)
                .min(max_label_width)
                .max(1);
            rect.width = width as f32;
            width
        } else {
            (rect.width.ceil().max(1.0) as u32).min(max_label_width)
        };
        Some((
            LabelCacheKey {
                text: label.to_string(),
                width: label_width,
                height: label_height,
                alignment,
                wrap,
            },
            rect,
            label_width,
            label_height,
        ))
    }

    fn cached_no_wrap_label_width(
        &mut self,
        label: &str,
        label_height: u32,
        max_label_width: u32,
    ) -> u32 {
        let key = LabelMetricsCacheKey {
            text: label.to_string(),
            label_height,
        };
        if let Some(width) = self.metrics_cache.get(&key) {
            return width.min(max_label_width).max(1);
        }

        let shaped_width = dolphin_text_width_no_wrap(
            self.font_system,
            self.text_buffer,
            label,
            self.max_font_size,
            self.max_line_height,
        );
        let width = (shaped_width.ceil().max(1.0) as u32)
            .saturating_add(TEXT_PADDING.saturating_mul(2));
        self.metrics_cache.insert(key, width);
        width.min(max_label_width).max(1)
    }

    fn resolve_label_pixels(
        &mut self,
        label: &str,
        key: &LabelCacheKey,
        label_width: u32,
        label_height: u32,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> Option<(Arc<[u8]>, LabelCacheOutcome)> {
        if let Some(pixels) = self.label_cache.get(key) {
            self.cache_hits += 1;
            return Some((pixels, LabelCacheOutcome::Hit));
        }

        self.cache_misses += 1;
        if self.raster_miss_budget == 0 {
            self.deferred += 1;
            return None;
        }
        self.raster_miss_budget -= 1;
        let raster_start = Instant::now();
        let label_pixels = self.rasterize_label(label, label_width, label_height, alignment, wrap);
        self.raster_us += raster_start.elapsed().as_micros();
        let pixels = self.label_cache.insert(key.clone(), label_pixels);
        Some((pixels, LabelCacheOutcome::Miss))
    }

    fn measure_label_cursor_x(
        &mut self,
        label: &str,
        rect: ViewRect,
        cursor: usize,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> f32 {
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return 0.0;
        }
        let max_label_width = text_atlas_max_label_width(self.width);
        let label_width = (rect.width.ceil().max(1.0) as u32).min(max_label_width);
        let label_height = rect.height.ceil().max(1.0) as u32;
        let attrs = Attrs::new().family(Family::SansSerif);
        let metrics =
            text_metrics_for_label_height(label_height, self.max_font_size, self.max_line_height);
        self.text_buffer.set_metrics(metrics);
        self.text_buffer.set_wrap(wrap.cosmic_wrap());
        self.text_buffer
            .set_size(Some(label_width as f32), Some(label_height as f32));
        self.text_buffer.set_text(
            label,
            &attrs,
            shaping_for_label(label, wrap),
            Some(alignment.cosmic_align()),
        );
        self.text_buffer.shape_until_scroll(self.font_system, false);
        let cursor = Cursor::new(0, normalized_text_cursor(label, cursor));
        let measured_x = self
            .text_buffer
            .cursor_position(&cursor)
            .map(|(x, _)| x)
            .or_else(|| self.text_buffer.layout_runs().next().map(|run| run.line_w))
            .unwrap_or(0.0);
        measured_x / (label_width as f32 / rect.width.max(1.0))
    }

    fn finish(mut self) -> TextFrame {
        let pending = std::mem::take(&mut self.pending_draws);
        let initial_atlas_height = self.atlas_cache.height;
        let label_cache_entry_limit = pending
            .len()
            .saturating_add(TEXT_LABEL_RECYCLE_CACHE_ENTRIES)
            .max(1);

        let mut atlas_reused: usize;
        let mut drawable = Vec::with_capacity(pending.len());
        let mut atlases = Vec::with_capacity(pending.len());
        let mut uploads = Vec::new();
        let mut reset_once = false;
        'build_atlas: loop {
            atlas_reused = 0;
            drawable.clear();
            atlases.clear();
            uploads.clear();

            for draw in pending.iter() {
                if let Some(atlas) = self.atlas_cache.entries.get(&draw.key).copied() {
                    atlas_reused += 1;
                    if draw.atlas_upload_required {
                        uploads.push(text_atlas_upload_from_draw(atlas, draw));
                    }
                    atlases.push(atlas);
                    drawable.push(draw.clone());
                    continue;
                }

                let Some(atlas) = self.atlas_cache.allocate(
                    text_atlas_guarded_extent(draw.label_width),
                    text_atlas_guarded_extent(draw.label_height),
                ) else {
                    if !reset_once {
                        reset_once = true;
                        self.atlas_cache.reset();
                        continue 'build_atlas;
                    }
                    self.deferred += 1;
                    continue;
                };
                self.atlas_cache.entries.insert(draw.key.clone(), atlas);
                uploads.push(text_atlas_upload_from_draw(atlas, draw));
                atlases.push(atlas);
                drawable.push(draw.clone());
            }
            // Growing the texture discards its old contents. Repack the live
            // frame so every retained slot is uploaded into the new texture,
            // and stale allocations cannot ratchet atlas memory upward.
            if !reset_once && self.atlas_cache.height != initial_atlas_height {
                reset_once = true;
                self.atlas_cache.reset();
                continue 'build_atlas;
            }
            break;
        }
        let height = self.atlas_cache.height.max(1);
        let mut pixels = self.atlas_pixels;
        pixels.clear();
        let vertices =
            text_vertices_for_pending(&drawable, &atlases, self.width, height, self.surface_size);
        if self
            .label_cache
            .evict_to_recent_entry_limit(label_cache_entry_limit)
        {
            self.atlas_cache
                .retain_label_cache_entries(self.label_cache);
        }
        let cache_entries = self.label_cache.len();
        let cache_bytes = self.label_cache.bytes();
        let atlas_bytes = (self.width * height) as usize;
        let atlas_uploads = uploads.len();
        TextFrame {
            vertices,
            pixels,
            uploads,
            width: self.width,
            height,
            stats: TextFrameStats {
                labels: self.labels,
                quads: drawable.len(),
                deferred: self.deferred,
                atlas_reused,
                atlas_uploads,
                atlas_upload_skips: 0,
                atlas_width: self.width,
                atlas_height: height,
                atlas_bytes,
                cache_hits: self.cache_hits,
                cache_misses: self.cache_misses,
                cache_entries,
                cache_bytes,
                swash_image_entries: 0,
                swash_outline_entries: 0,
                swash_resets: 0,
                raster_us: self.raster_us,
            },
        }
    }
}
