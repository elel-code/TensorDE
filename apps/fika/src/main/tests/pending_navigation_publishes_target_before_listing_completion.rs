use std::path::PathBuf;

#[test]
fn pending_navigation_publishes_target_and_hides_the_previous_model() {
    let mut scene = test_scene(
        vec![test_entry("Desktop", true), test_entry("notes.txt", false)],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let pane = ShellPaneId::SLOT_0;
    let target = PathBuf::from("/fixture/Desktop");
    let source = scene.panes[pane].path.clone();
    assert!(scene.panes[pane].selection.apply_navigation(0, false));

    let before = ShellRenderDirtyKey::from_scene(&scene, size);
    assert!(scene.begin_pane_navigation(pane, target.clone(), size));

    assert_eq!(scene.location_label_for_pane(pane), target.display().to_string());
    assert_eq!(scene.panes[pane].pending_path.as_ref(), Some(&target));
    assert!(scene.panes[pane].selection.selected.is_empty());
    let projection = scene.pane_projection(pane, size).unwrap();
    assert_eq!(projection.view.path, target.as_path());
    assert!(projection.view.entries.is_empty());
    assert!(projection.visible_items.is_empty());
    assert_ne!(before, ShellRenderDirtyKey::from_scene(&scene, size));

    assert!(scene.cancel_pane_navigation(pane));
    let restored = scene.pane_projection(pane, size).unwrap();
    assert_eq!(restored.view.path, source.as_path());
    assert_eq!(restored.visible_items.len(), 2);
}

#[test]
fn pending_navigation_commits_only_its_matching_completion() {
    let mut scene = test_scene(vec![test_entry("Desktop", true)], ShellViewMode::Details);
    let size = PhysicalSize::new(720, 420);
    let pane = ShellPaneId::SLOT_0;
    let target = PathBuf::from("/fixture/Desktop");
    let stale = PathBuf::from("/fixture/stale");

    assert!(scene.begin_pane_navigation(pane, target.clone(), size));
    assert!(!scene.complete_pane_navigation(
        pane,
        stale,
        vec![test_entry("wrong.txt", false)],
        size,
    ));
    assert_eq!(scene.panes[pane].pending_path.as_ref(), Some(&target));

    assert!(scene.complete_pane_navigation(
        pane,
        target.clone(),
        vec![test_entry("welcome.txt", false)],
        size,
    ));
    assert_eq!(scene.panes[pane].path, target);
    assert!(scene.panes[pane].pending_path.is_none());
    assert_eq!(scene.panes[pane].entries[0].name.as_ref(), "welcome.txt");
}

#[test]
fn shared_navigation_completion_keeps_generation_and_history_atomic() {
    let mut scene = test_scene(vec![test_entry("Desktop", true)], ShellViewMode::Icons);
    let size = PhysicalSize::new(720, 420);
    let pane = ShellPaneId::SLOT_0;
    let source = scene.panes[pane].path.clone();
    let target = PathBuf::from("/fixture/Desktop");

    assert!(scene.begin_pane_navigation(pane, target.clone(), size));
    let stale = ShellAsyncNavigationCompletion {
        generation: 3,
        pane,
        source_path: source.clone(),
        target_path: target.clone(),
        history: ShellNavigationHistoryUpdate::Push,
        reason: "test",
        result: Ok(vec![test_entry("stale.txt", false)]),
    };
    assert!(!crate::app_actions::apply_navigation_completion(
        &mut scene,
        &[4, 0],
        stale,
        size,
    ));
    assert_eq!(scene.panes[pane].pending_path.as_ref(), Some(&target));
    assert!(scene.pane_history(pane).back.is_empty());

    let completion = ShellAsyncNavigationCompletion {
        generation: 4,
        pane,
        source_path: source.clone(),
        target_path: target.clone(),
        history: ShellNavigationHistoryUpdate::Push,
        reason: "test",
        result: Ok(vec![test_entry("welcome.txt", false)]),
    };
    assert!(crate::app_actions::apply_navigation_completion(
        &mut scene,
        &[4, 0],
        completion,
        size,
    ));
    assert_eq!(scene.panes[pane].path, target);
    assert_eq!(scene.pane_history(pane).back.last(), Some(&source));
}
