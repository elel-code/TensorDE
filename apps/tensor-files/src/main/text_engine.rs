//! Renderer-independent shaped-text caches and reusable staging state.

use crate::*;

pub(crate) struct TextEngine {
    pub(crate) font_system: FontSystem,
    pub(crate) swash_cache: SwashCache,
    pub(crate) text_buffer: Buffer,
    pub(crate) details_texts: DetailsTextCache,
    pub(crate) pane_status_texts: PaneStatusTextCache,
    pub(crate) label_texts: LabelTextInterner,
    pub(crate) label_cache: LabelRasterCache,
    pub(crate) metrics_cache: LabelMetricsCache,
    pub(crate) atlas_cache: TextAtlasFrameCache,
    pub(crate) staging: TextFrameStaging,
}

impl TextEngine {
    pub(crate) fn new() -> Self {
        let mut font_system = FontSystem::new();
        let mut text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT),
        );
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            text_buffer,
            details_texts: DetailsTextCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES),
            pane_status_texts: PaneStatusTextCache::new(),
            label_texts: LabelTextInterner::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES),
            label_cache: LabelRasterCache::new(TEXT_LABEL_CACHE_MAX_BYTES),
            metrics_cache: LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES),
            atlas_cache: TextAtlasFrameCache::default(),
            staging: TextFrameStaging::default(),
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.label_cache.begin_frame();
        self.metrics_cache.begin_frame();
    }

    pub(crate) fn trim_caches(&mut self) -> (usize, usize, bool) {
        let image_entries = self.swash_cache.image_cache.len();
        let outline_entries = self.swash_cache.outline_command_cache.len();
        let reset = image_entries > TEXT_SWASH_IMAGE_CACHE_MAX_ENTRIES
            || outline_entries > TEXT_SWASH_OUTLINE_CACHE_MAX_ENTRIES;
        if reset {
            self.swash_cache = SwashCache::new();
        }
        (image_entries, outline_entries, reset)
    }

    pub(crate) fn take_frame_staging(&mut self) -> TextFrameStaging {
        std::mem::take(&mut self.staging)
    }

    pub(crate) fn recycle_frame(&mut self, frame: &mut TextFrame) {
        let mut staging = std::mem::take(&mut self.staging);
        std::mem::swap(&mut staging.pending_draws, &mut frame.pending_draws);
        std::mem::swap(&mut staging.drawable_indices, &mut frame.drawable_indices);
        std::mem::swap(&mut staging.atlases, &mut frame.atlases);
        std::mem::swap(&mut staging.vertices, &mut frame.vertices);
        std::mem::swap(&mut staging.pixels, &mut frame.pixels);
        std::mem::swap(&mut staging.uploads, &mut frame.uploads);
        staging.clear();
        self.staging = staging;
    }
}
