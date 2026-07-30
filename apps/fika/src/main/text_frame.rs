use crate::*;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TextFrameStats {
    pub(crate) labels: usize,
    pub(crate) quads: usize,
    pub(crate) deferred: usize,
    pub(crate) atlas_reused: usize,
    pub(crate) atlas_uploads: usize,
    pub(crate) atlas_upload_skips: usize,
    pub(crate) atlas_width: u32,
    pub(crate) atlas_height: u32,
    pub(crate) atlas_bytes: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_entries: usize,
    pub(crate) cache_bytes: usize,
    pub(crate) swash_image_entries: usize,
    pub(crate) swash_outline_entries: usize,
    pub(crate) swash_resets: usize,
    pub(crate) raster_us: u128,
}
pub(crate) struct TextFrame {
    pub(crate) vertices: Vec<TextVertex>,
    pub(crate) pixels: Vec<u8>,
    pub(crate) uploads: Vec<TextAtlasUpload>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stats: TextFrameStats,
}
pub(crate) const TEXT_ATLAS_GUARD_TEXELS: u32 = 1;
#[derive(Clone, Debug)]
pub(crate) struct PendingTextDraw {
    pub(crate) key: LabelCacheKey,
    pub(crate) pixels: Arc<[u8]>,
    pub(crate) atlas_upload_required: bool,
    pub(crate) screen: ViewRect,
    pub(crate) rect: ViewRect,
    pub(crate) label_width: u32,
    pub(crate) label_height: u32,
    pub(crate) color: TextColor,
}
#[derive(Clone, Debug)]
pub(crate) struct TextAtlasUpload {
    pub(crate) atlas: AtlasRect,
    pub(crate) pixels: Arc<[u8]>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TextAtlasUploadKey {
    pub(crate) atlas_x: u32,
    pub(crate) atlas_y: u32,
    pub(crate) atlas_width: u32,
    pub(crate) atlas_height: u32,
    pub(crate) upload_width: u32,
    pub(crate) upload_height: u32,
    pub(crate) pixels_hash: u64,
}
impl TextAtlasUploadKey {
    pub(crate) fn from_upload(upload: &TextAtlasUpload) -> Self {
        Self {
            atlas_x: upload.atlas.x as u32,
            atlas_y: upload.atlas.y as u32,
            atlas_width: upload.atlas.width as u32,
            atlas_height: upload.atlas.height as u32,
            upload_width: upload.width,
            upload_height: upload.height,
            pixels_hash: hash_bytes_with_len(upload.pixels.as_ref()),
        }
    }
}
pub(crate) fn text_atlas_max_label_width(atlas_width: u32) -> u32 {
    atlas_width
        .saturating_sub(TEXT_PADDING * 2 + TEXT_ATLAS_GUARD_TEXELS * 2)
        .max(1)
}
pub(crate) fn text_atlas_guarded_extent(extent: u32) -> u32 {
    extent + TEXT_ATLAS_GUARD_TEXELS * 2
}
pub(crate) fn padded_text_atlas_pixels(
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
) -> (Arc<[u8]>, u32, u32) {
    if TEXT_ATLAS_GUARD_TEXELS == 0 || width == 0 || height == 0 {
        return (pixels, width, height);
    }

    let guard = TEXT_ATLAS_GUARD_TEXELS;
    let padded_width = text_atlas_guarded_extent(width);
    let padded_height = text_atlas_guarded_extent(height);
    let mut padded = vec![0; (padded_width * padded_height) as usize];
    for y in 0..padded_height {
        let src_y = y.saturating_sub(guard).min(height.saturating_sub(1));
        for x in 0..padded_width {
            let src_x = x.saturating_sub(guard).min(width.saturating_sub(1));
            padded[(y * padded_width + x) as usize] = pixels[(src_y * width + src_x) as usize];
        }
    }

    (padded.into(), padded_width, padded_height)
}
