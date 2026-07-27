fn text_atlas_upload_from_draw(atlas: AtlasRect, draw: &PendingTextDraw) -> TextAtlasUpload {
    let (pixels, width, height) = padded_text_atlas_pixels(
        Arc::clone(&draw.pixels),
        draw.label_width,
        draw.label_height,
    );
    TextAtlasUpload {
        atlas,
        pixels,
        width,
        height,
    }
}
#[derive(Clone, Debug)]
struct TextAtlasFrameCache {
    entries: HashMap<LabelCacheKey, AtlasRect>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}
impl Default for TextAtlasFrameCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            width: TEXT_ATLAS_WIDTH,
            height: 1,
            cursor_x: TEXT_PADDING,
            cursor_y: TEXT_PADDING,
            row_height: 0,
        }
    }
}
impl TextAtlasFrameCache {
    fn reset(&mut self) {
        self.entries.clear();
        self.height = 1;
        self.cursor_x = TEXT_PADDING;
        self.cursor_y = TEXT_PADDING;
        self.row_height = 0;
    }

    fn allocate(&mut self, label_width: u32, label_height: u32) -> Option<AtlasRect> {
        if self.cursor_x + label_width + TEXT_PADDING > self.width {
            self.cursor_x = TEXT_PADDING;
            self.cursor_y += self.row_height.max(1);
            self.row_height = 0;
        }

        let x = self.cursor_x;
        let y = self.cursor_y;
        let needed_height = y + label_height + TEXT_PADDING;
        if needed_height > TEXT_ATLAS_MAX_HEIGHT {
            return None;
        }
        self.cursor_x += label_width + TEXT_PADDING;
        self.row_height = self.row_height.max(label_height + TEXT_PADDING);
        self.ensure_height(needed_height);

        Some(AtlasRect {
            x: x as f32,
            y: y as f32,
            width: label_width as f32,
            height: label_height as f32,
        })
    }

    fn ensure_height(&mut self, needed_height: u32) {
        if needed_height <= self.height {
            return;
        }
        self.height = needed_height.next_power_of_two();
    }

    fn retain_label_cache_entries(&mut self, label_cache: &LabelRasterCache) {
        self.entries.retain(|key, _| label_cache.contains_key(key));
        if self.entries.capacity() > self.entries.len().saturating_mul(2).max(64) {
            self.entries.shrink_to_fit();
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LabelAlignment {
    Start,
    Center,
    End,
}
impl LabelAlignment {
    fn cosmic_align(self) -> Align {
        match self {
            Self::Start => Align::Left,
            Self::Center => Align::Center,
            Self::End => Align::Right,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LabelWrap {
    None,
    WordOrGlyph,
}
impl LabelWrap {
    const fn cosmic_wrap(self) -> Wrap {
        match self {
            Self::None => Wrap::None,
            Self::WordOrGlyph => Wrap::WordOrGlyph,
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LabelCacheKey {
    text: String,
    width: u32,
    height: u32,
    alignment: LabelAlignment,
    wrap: LabelWrap,
}
#[derive(Clone, Debug)]
struct CachedLabel {
    pixels: Arc<[u8]>,
    bytes: usize,
    last_used_frame: u64,
}
#[derive(Debug)]
struct LabelRasterCache {
    entries: HashMap<LabelCacheKey, CachedLabel>,
    frame: u64,
    bytes: usize,
    max_bytes: usize,
}
impl LabelRasterCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            bytes: 0,
            max_bytes,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &LabelCacheKey) -> Option<Arc<[u8]>> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(Arc::clone(&entry.pixels))
    }

    fn insert(&mut self, key: LabelCacheKey, pixels: Vec<u8>) -> Arc<[u8]> {
        let bytes = pixels.len();
        let pixels = Arc::<[u8]>::from(pixels);
        if let Some(old) = self.entries.insert(
            key.clone(),
            CachedLabel {
                pixels: Arc::clone(&pixels),
                bytes,
                last_used_frame: self.frame,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.evict_if_needed(&key);
        pixels
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn contains_key(&self, key: &LabelCacheKey) -> bool {
        self.entries.contains_key(key)
    }

    fn evict_to_recent_entry_limit(&mut self, max_entries: usize) -> bool {
        let max_entries = max_entries.max(1);
        let mut evicted = false;
        while self.entries.len() > max_entries {
            let Some(victim) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                evicted = true;
            }
        }
        if evicted && self.entries.capacity() > self.entries.len().saturating_mul(2).max(64) {
            self.entries.shrink_to_fit();
        }
        evicted
    }

    fn evict_if_needed(&mut self, protected: &LabelCacheKey) {
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LabelMetricsCacheKey {
    text: String,
    label_height: u32,
}
#[derive(Clone, Debug)]
struct CachedLabelMetrics {
    natural_width: u32,
    last_used_frame: u64,
}
#[derive(Debug)]
struct LabelMetricsCache {
    entries: HashMap<LabelMetricsCacheKey, CachedLabelMetrics>,
    frame: u64,
    max_entries: usize,
}
impl LabelMetricsCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            max_entries,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &LabelMetricsCacheKey) -> Option<u32> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(entry.natural_width)
    }

    fn insert(&mut self, key: LabelMetricsCacheKey, natural_width: u32) {
        self.entries.insert(
            key.clone(),
            CachedLabelMetrics {
                natural_width,
                last_used_frame: self.frame,
            },
        );
        self.evict_if_needed(&key);
    }

    fn evict_if_needed(&mut self, protected: &LabelMetricsCacheKey) {
        while self.entries.len() > self.max_entries && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.entries.remove(&victim);
        }
    }
}
struct TextHitTestRuntime {
    font_system: FontSystem,
    text_buffer: Buffer,
}
impl TextHitTestRuntime {
    fn new() -> Self {
        let mut font_system = FontSystem::new();
        let text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT),
        );
        Self {
            font_system,
            text_buffer,
        }
    }

    fn cursor_for_offset(
        &mut self,
        label: &str,
        rect: ViewRect,
        offset_x: f32,
        alignment: LabelAlignment,
        wrap: LabelWrap,
        scale_factor: f32,
    ) -> usize {
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 || offset_x <= 0.0 {
            return 0;
        }
        let label_width = rect.width.ceil().max(1.0) as u32;
        let label_height = rect.height.ceil().max(1.0) as u32;
        let max_line_height = (TEXT_LINE_HEIGHT * scale_factor).round().max(1.0);
        let max_font_size = (TEXT_FONT_SIZE * max_line_height / TEXT_LINE_HEIGHT).max(1.0);
        let metrics = text_metrics_for_label_height(label_height, max_font_size, max_line_height);
        let attrs = Attrs::new().family(Family::SansSerif);

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
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);

        let scale_x = label_width as f32 / rect.width.max(1.0);
        let x = (offset_x * scale_x).clamp(0.0, label_width as f32);
        let y = label_height as f32 / 2.0;
        self.text_buffer
            .hit(x, y)
            .map(|cursor| normalized_text_cursor(label, cursor.index))
            .unwrap_or_else(|| {
                if offset_x >= rect.width {
                    label.len()
                } else {
                    0
                }
            })
    }

    fn icons_filename_line_count(
        &mut self,
        label: &str,
        available_width: f32,
        max_lines: usize,
        font_size: f32,
        line_height: f32,
    ) -> usize {
        dolphin_icons_filename_line_count(
            &mut self.font_system,
            &mut self.text_buffer,
            label,
            available_width,
            max_lines,
            font_size,
            line_height,
        )
    }

    fn no_wrap_width(&mut self, label: &str, font_size: f32, line_height: f32) -> f32 {
        dolphin_text_width_no_wrap(
            &mut self.font_system,
            &mut self.text_buffer,
            label,
            font_size,
            line_height,
        )
    }

    fn cursor_x(
        &mut self,
        label: &str,
        cursor: usize,
        layout: TextCursorLayout,
    ) -> f32 {
        let TextCursorLayout {
            rect,
            alignment,
            wrap,
            max_font_size,
            max_line_height,
        } = layout;
        if label.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return 0.0;
        }
        let label_width = rect.width.ceil().max(1.0) as u32;
        let label_height = rect.height.ceil().max(1.0) as u32;
        let attrs = Attrs::new().family(Family::SansSerif);
        let metrics = text_metrics_for_label_height(label_height, max_font_size, max_line_height);

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
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);

        let cursor = Cursor::new(0, normalized_text_cursor(label, cursor));
        let measured_x = self
            .text_buffer
            .cursor_position(&cursor)
            .map(|(x, _)| x)
            .or_else(|| self.text_buffer.layout_runs().next().map(|run| run.line_w))
            .unwrap_or(0.0);
        measured_x / (label_width as f32 / rect.width.max(1.0))
    }
}

#[derive(Clone, Copy)]
struct TextCursorLayout {
    rect: ViewRect,
    alignment: LabelAlignment,
    wrap: LabelWrap,
    max_font_size: f32,
    max_line_height: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LabelCacheOutcome {
    Hit,
    Miss,
    Deferred,
    Skipped,
}
struct TextFrameBuilder<'a> {
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    text_buffer: &'a mut Buffer,
    label_cache: &'a mut LabelRasterCache,
    metrics_cache: &'a mut LabelMetricsCache,
    atlas_cache: &'a mut TextAtlasFrameCache,
    surface_size: PhysicalSize<u32>,
    max_font_size: f32,
    max_line_height: f32,
    pending_draws: Vec<PendingTextDraw>,
    width: u32,
    labels: usize,
    cache_hits: usize,
    cache_misses: usize,
    deferred: usize,
    raster_miss_budget: usize,
    raster_us: u128,
    atlas_pixels: Vec<u8>,
    text_midline_shift: f32,
}
include!("text_frame_builder/builder.rs");
include!("text_frame_builder/rasterize.rs");
