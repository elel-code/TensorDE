use std::path::Path;

use crate::ui::metrics::FILE_MANAGER_FOLDER_PREVIEW_MAX_IMAGES;
use crate::ui::render::gpu::hash_bytes_with_len;

pub(crate) const FOLDER_PREVIEW_LAYOUT_VERSION: u64 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileManagerDirectoryPreviewLayout {
    pub(crate) folder_size: u32,
    pub(crate) top_margin: u32,
    pub(crate) bottom_margin: u32,
    pub(crate) left_margin: u32,
    pub(crate) right_margin: u32,
    pub(crate) spacing: u32,
    pub(crate) segment_width: u32,
    pub(crate) segment_height: u32,
    pub(crate) border_stroke_width: u32,
}

impl FileManagerDirectoryPreviewLayout {
    pub(crate) fn new(folder_size: u32) -> Option<Self> {
        let folder_size = folder_size.clamp(16, 256);
        let spacing = 1;
        let tiles = 2;
        let top_margin = folder_size * 30 / 100;
        let bottom_margin = folder_size / 6;
        let left_margin = folder_size / 13;
        let right_margin = left_margin;
        let segment_width = (folder_size - left_margin - right_margin + spacing) / tiles - spacing;
        let segment_height = (folder_size - top_margin - bottom_margin + spacing) / tiles - spacing;
        if segment_width < 5 || segment_height < 5 {
            return None;
        }
        let border_stroke_width = ((folder_size as f32 / 170.0) + 0.5).floor() as u32;
        Some(Self {
            folder_size,
            top_margin,
            bottom_margin,
            left_margin,
            right_margin,
            spacing,
            segment_width,
            segment_height,
            border_stroke_width,
        })
    }

    pub(crate) fn one_tile_slot(self) -> FolderPreviewThumbnailSlot {
        FolderPreviewThumbnailSlot {
            x: self.left_margin,
            y: self.top_margin,
            width: self
                .folder_size
                .saturating_sub(self.left_margin + self.right_margin),
            height: self
                .folder_size
                .saturating_sub(self.top_margin + self.bottom_margin),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FolderPreviewThumbnailSlot {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn folder_preview_thumbnail_slots(
    count: usize,
    layout: FileManagerDirectoryPreviewLayout,
) -> Vec<FolderPreviewThumbnailSlot> {
    let count = count.min(FILE_MANAGER_FOLDER_PREVIEW_MAX_IMAGES);
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![layout.one_tile_slot()];
    }
    let row2_y = layout.top_margin + layout.segment_height + layout.spacing;
    if count == 3 {
        let available_width = layout
            .folder_size
            .saturating_sub(layout.left_margin + layout.right_margin);
        let centered_x =
            layout.left_margin + available_width.saturating_sub(layout.segment_width) / 2;
        return vec![
            FolderPreviewThumbnailSlot {
                x: layout.left_margin,
                y: layout.top_margin,
                width: layout.segment_width,
                height: layout.segment_height,
            },
            FolderPreviewThumbnailSlot {
                x: layout.left_margin + layout.segment_width + layout.spacing,
                y: layout.top_margin,
                width: layout.segment_width,
                height: layout.segment_height,
            },
            FolderPreviewThumbnailSlot {
                x: centered_x,
                y: row2_y,
                width: layout.segment_width,
                height: layout.segment_height,
            },
        ];
    }
    let mut slots = Vec::with_capacity(count);
    let mut x = layout.left_margin;
    let mut y = layout.top_margin;
    for _ in 0..count {
        slots.push(FolderPreviewThumbnailSlot {
            x,
            y,
            width: layout.segment_width,
            height: layout.segment_height,
        });
        x += layout.segment_width + layout.spacing;
        if x > layout.folder_size - layout.right_margin - layout.segment_width {
            x = layout.left_margin;
            y += layout.segment_height + layout.spacing;
        }
    }
    slots
}

pub(crate) fn folder_preview_directory_seed(directory: &Path) -> u64 {
    hash_bytes_with_len(directory.to_string_lossy().as_bytes())
}

pub(crate) fn folder_preview_thumbnail_angle(seed: u64, index: usize) -> i32 {
    let mut value = seed ^ ((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value % 17) as i32 - 8
}
