//! Renderer-independent icon resolution and residency snapshot state.

use std::collections::HashMap;

use crate::{
    FileIconResolver, IconFrame, IconFrameStaging, IconGpuUploadKey, IconRounding,
    ThumbnailSourceResolver,
};

pub(crate) trait IconGpuResidentLookup {
    fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry>;
}

/// Snapshot of resident GPU icon sizes at frame start.
#[derive(Clone, Debug, Default)]
pub(crate) struct IconGpuResidentIndex {
    pub(crate) entries: HashMap<IconGpuUploadKey, IconGpuResidentEntry>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IconGpuResidentEntry {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_width: u32,
    pub(crate) content_height: u32,
    pub(crate) content_hash: u64,
    pub(crate) rounding: Option<IconRounding>,
}

impl IconGpuResidentIndex {
    pub(crate) fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry> {
        self.entries.get(key).copied()
    }
}

impl IconGpuResidentLookup for IconGpuResidentIndex {
    fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry> {
        self.get(key)
    }
}

pub(crate) enum IconGpuResidentSource<'a> {
    Owned(IconGpuResidentIndex),
    Borrowed(&'a dyn IconGpuResidentLookup),
}

impl IconGpuResidentSource<'_> {
    pub(crate) fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry> {
        match self {
            Self::Owned(index) => index.get(key),
            Self::Borrowed(index) => index.get(key),
        }
    }
}

pub(crate) struct IconEngine {
    pub(crate) resolver: FileIconResolver,
    pub(crate) thumbnails: ThumbnailSourceResolver,
    pub(crate) staging: IconFrameStaging,
}

impl IconEngine {
    pub(crate) fn new() -> Self {
        Self {
            resolver: FileIconResolver::new(),
            thumbnails: ThumbnailSourceResolver::new(),
            staging: IconFrameStaging::default(),
        }
    }

    pub(crate) fn take_frame_staging(&mut self) -> IconFrameStaging {
        std::mem::take(&mut self.staging)
    }

    pub(crate) fn recycle_frame(&mut self, frame: &mut IconFrame) {
        // Vulkan upload consumes these cold sources. Clear any source left by
        // an unsupported upload before returning the slot vector to staging.
        for slot in &mut frame.slots {
            slot.source = None;
            let _ = slot.dmabuf.take();
        }
        let mut staging = std::mem::take(&mut self.staging);
        std::mem::swap(&mut staging.slot_by_identity, &mut frame.slot_by_identity);
        std::mem::swap(&mut staging.slots, &mut frame.slots);
        std::mem::swap(&mut staging.draws, &mut frame.draws);
        std::mem::swap(&mut staging.overlay_draws, &mut frame.overlay_draws);
        std::mem::swap(&mut staging.content_batches, &mut frame.content_batches);
        std::mem::swap(&mut staging.overlay_batches, &mut frame.overlay_batches);
        std::mem::swap(&mut staging.content_vertices, &mut frame.content_vertices);
        std::mem::swap(&mut staging.overlay_vertices, &mut frame.overlay_vertices);
        std::mem::swap(
            &mut staging.batch_draw_indices,
            &mut frame.batch_draw_indices,
        );
        std::mem::swap(&mut staging.batch_slot_order, &mut frame.batch_slot_order);
        staging.clear();
        self.staging = staging;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_resident_lookup_matches_explicit_snapshot() {
        let key = IconGpuUploadKey::named_asset("folder".to_string(), 64);
        let entry = IconGpuResidentEntry {
            width: 64,
            height: 64,
            content_width: 48,
            content_height: 52,
            content_hash: 0xfeed,
            rounding: None,
        };
        let index = IconGpuResidentIndex {
            entries: HashMap::from([(key.clone(), entry)]),
        };

        let borrowed = IconGpuResidentSource::Borrowed(&index);
        let owned = IconGpuResidentSource::Owned(index.clone());
        let borrowed_entry = borrowed.get(&key).unwrap();
        let owned_entry = owned.get(&key).unwrap();
        assert_eq!(borrowed_entry.content_hash, owned_entry.content_hash);
        assert_eq!(borrowed_entry.content_width, owned_entry.content_width);
        assert_eq!(borrowed_entry.content_height, owned_entry.content_height);
    }
}
