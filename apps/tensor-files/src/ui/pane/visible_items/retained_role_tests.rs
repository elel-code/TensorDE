use std::path::Path;
use std::sync::Arc;

use tensor_files_core::EntryData;

use super::*;

struct PreparedEntry {
    entry_index: Option<usize>,
    slot_id: u64,
}

impl ShellVisibleSlotItem for PreparedEntry {
    fn visible_slot_entry_index(&self) -> Option<usize> {
        self.entry_index
    }

    fn set_visible_slot_id(&mut self, slot_id: u64) {
        self.slot_id = slot_id;
    }
}

#[test]
fn metadata_replacement_refreshes_cached_entry_and_retained_icon_role() {
    let directory = Path::new("/tmp/visible-items");
    let original = Entry::new(EntryData {
        name: Arc::from("payload.bin"),
        name_width_units: 0,
        target_path: None,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: false,
    });
    let replacement = Entry::new(EntryData {
        mime_type: Some(Arc::from("text/plain")),
        ..(*original).clone()
    });
    let mut pool = ShellVisibleItemSlotPool::default();
    let mut prepared = [PreparedEntry {
        entry_index: Some(0),
        slot_id: 0,
    }];

    pool.update_visible_item_slots(directory, std::slice::from_ref(&original), &mut prepared);
    let epoch = pool.visible_epoch();
    let stats = pool.update_visible_item_slots(
        directory,
        std::slice::from_ref(&replacement),
        &mut prepared,
    );

    assert_eq!(stats.reused, stats.active);
    assert_eq!(pool.visible_epoch(), epoch);
    assert!(Entry::ptr_eq(
        pool.projection_cache[0].entry.as_ref().unwrap(),
        &replacement
    ));
    assert!(matches!(
        pool.retained_icon_role_for_entry(&replacement)
            .map(|role| &role.kind),
        Some(crate::ui::icon_roles::FileIconKind::Mime { mime })
            if mime.as_ref() == "text/plain"
    ));
    let (retained_path, retained_role) = pool
        .retained_visual_for_entry(&replacement)
        .expect("retained visual should resolve from one entity lookup");
    assert_eq!(retained_path.as_ref(), directory.join("payload.bin"));
    assert!(matches!(
        retained_role.map(|role| &role.kind),
        Some(crate::ui::icon_roles::FileIconKind::Mime { mime })
            if mime.as_ref() == "text/plain"
    ));

    pool.update_visible_item_slots(directory, std::slice::from_ref(&replacement), &mut prepared);
    assert!(Entry::ptr_eq(
        pool.projection_cache[0].entry.as_ref().unwrap(),
        &replacement
    ));
}
