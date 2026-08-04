pub fn normalize_viewport_extent(extent: f32) -> f32 {
    extent.max(1.0).floor()
}

fn viewport_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

fn apply_pane_sort(pane: &mut PaneState, sort: SortDescriptor) -> Vec<DirectoryModelSignal> {
    let signals = pane.model.set_sort(sort);
    if !signals.is_empty() {
        let fallback_id = pane.model.get(0).map(|entry| entry.id);
        let model = &pane.model;
        pane.selection
            .retain_existing_by(|id| model.index_of_id(id).is_some(), fallback_id);
        pane.view.reset_scroll();
    }
    signals
}

fn selected_paths_from_model(pane: &PaneState) -> Vec<PathBuf> {
    if pane.selection.is_all_selected() {
        return (0..pane.model.len())
            .filter(|index| {
                pane.model
                    .get(*index)
                    .is_some_and(|entry| !pane.selection.is_excluded(entry.id))
            })
            .filter_map(|index| pane.model.path_for_index(index))
            .collect();
    }

    pane.selection
        .selected_ids()
        .iter()
        .filter_map(|id| path_for_selection_id(pane, *id))
        .collect()
}

fn path_for_selection_id(pane: &PaneState, id: ItemId) -> Option<PathBuf> {
    pane.model
        .index_of_id(id)
        .and_then(|index| pane.model.path_for_index(index))
}

