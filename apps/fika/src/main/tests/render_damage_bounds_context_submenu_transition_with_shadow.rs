    #[test]
    fn render_damage_bounds_context_submenu_transition_with_shadow() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let scale = scene.ui_scale();
        let menu = scene.context_menu.as_mut().unwrap();
        let create_new_row = context_menu_items(menu)
            .iter()
            .position(|item| item.submenu == Some(ShellContextSubmenu::CreateNew))
            .expect("blank context menu should expose create-new submenu");
        menu.hovered_row = Some(create_new_row);
        menu.active_submenu = Some(ShellContextSubmenu::CreateNew);
        menu.active_submenu_row = Some(create_new_row);
        let raw_submenu_rect = context_menu_submenu_rect(menu, size, scale).unwrap();
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let submenu = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &submenu, false);
        let bounds = damage.bounds.unwrap();

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert!(damage.rect_count >= 2);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
        assert!(bounds.x <= raw_submenu_rect.x);
        assert!(bounds.y <= raw_submenu_rect.y);
        assert!(bounds.right() >= raw_submenu_rect.right());
        assert!(bounds.bottom() >= raw_submenu_rect.bottom());
    }

    #[test]
    fn render_damage_bounds_drop_menu_hover_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.drop_menu = Some(ShellDropMenu::new(
            vec![PathBuf::from("/tmp/source.txt")],
            PathBuf::from("/tmp"),
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.drop_menu.as_mut().unwrap().hovered_row = Some(0);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_bounds_drop_menu_open_and_close() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let closed = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.drop_menu = Some(ShellDropMenu::new(
            vec![PathBuf::from("/tmp/source.txt")],
            PathBuf::from("/tmp"),
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let opened = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let open_damage = ShellRenderDamage::between(Some(&closed), &opened, false);
        let close_damage = ShellRenderDamage::between(Some(&opened), &closed, false);

        assert_eq!(open_damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(open_damage.rect_count, 1);
        assert_eq!(
            open_damage.bounds,
            opened.drop_menu.as_ref().map(|state| state.overlay_rect)
        );
        assert!(open_damage.area_px < rect_area(full_surface_rect(size)));
        assert_eq!(close_damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(
            close_damage.bounds,
            opened.drop_menu.as_ref().map(|state| state.overlay_rect)
        );
    }

    #[test]
    fn render_damage_bounds_dnd_hover_pane_item_transition() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.dnd_hover_target = Some(ShellDropTarget::PaneItem {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/alpha"),
            is_dir: true,
        });
        scene.dnd_hover_changes += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_bounds_compact_dnd_hover_pane_item_to_visual_rect() {
        let mut scene = test_scene(
            vec![
                test_entry("very-long-folder-name-in-the-same-compact-column", true),
                test_entry("src", true),
            ],
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(900, 420);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let layout = projections[0].visible_items[1].layout;
        let item_rect = pane_content_rect_to_screen(layout.item_rect, &projections[0]);
        let visual_rect = pane_content_rect_to_screen(layout.visual_rect, &projections[0]);
        assert!(visual_rect.width < item_rect.width);
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.dnd_hover_target = Some(ShellDropTarget::PaneItem {
            pane: ShellPaneId::SLOT_0,
            index: 1,
            path: PathBuf::from("/tmp/src"),
            is_dir: true,
        });
        scene.dnd_hover_changes += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.bounds, Some(visual_rect));
    }

    #[test]
    fn render_damage_bounds_details_dnd_hover_pane_item_full_row() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Details);
        let size = PhysicalSize::new(900, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let row_rect = pane_content_rect_to_screen(
            projections[0].visible_items[0].layout.item_rect,
            &projections[0],
        );
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.dnd_hover_target = Some(ShellDropTarget::PaneItem {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/alpha"),
            is_dir: true,
        });
        scene.dnd_hover_changes += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        let bounds = damage.bounds.expect("dnd hover should produce damage");
        assert_eq!(bounds, row_rect);
    }

    #[test]
    fn render_damage_bounds_dnd_hover_places_gap_transition() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "A", "Alpha", PathBuf::from("/tmp/place-alpha"), true),
            ShellPlace::new("", "B", "Beta", PathBuf::from("/tmp/place-beta"), true),
        ];
        let size = PhysicalSize::new(700, 360);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.dnd_hover_target = Some(ShellDropTarget::PlacesGap { index: 2 });
        scene.dnd_hover_changes += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn damage_scissor_rect_inflates_fractional_bounds_and_clamps_to_surface() {
        let scissor = damage_scissor_rect(
            ViewRect {
                x: -2.4,
                y: 9.2,
                width: 17.7,
                height: 13.1,
            },
            PhysicalSize::new(20, 20),
        )
        .unwrap();

        assert_eq!(
            scissor,
            DamageScissorRect {
                x: 0,
                y: 9,
                width: 16,
                height: 11,
            }
        );
    }

    #[test]
    fn damage_scissor_rect_rejects_empty_or_outside_bounds() {
        assert_eq!(
            damage_scissor_rect(
                ViewRect {
                    x: 40.0,
                    y: 8.0,
                    width: 4.0,
                    height: 4.0,
                },
                PhysicalSize::new(20, 20),
            ),
            None
        );
        assert_eq!(
            damage_scissor_rect(
                ViewRect {
                    x: 4.0,
                    y: 4.0,
                    width: 0.0,
                    height: 10.0,
                },
                PhysicalSize::new(20, 20),
            ),
            None
        );
    }

    #[test]
    fn retained_scene_vertices_cover_full_surface_texture() {
        let vertices = retained_scene_vertices();

        for position in [[-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0], [1.0, 1.0]] {
            assert!(vertices.iter().any(|vertex| vertex.position == position));
        }
        for uv in [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]] {
            assert!(vertices.iter().any(|vertex| vertex.uv == uv));
        }
    }

    #[test]
    fn retained_scene_present_pass_preserves_per_pixel_alpha() {
        let vertices = retained_scene_vertices();
        assert!(vertices.iter().all(|vertex| vertex.color == [1.0; 4]));
    }

    #[test]
    fn render_dirty_key_tracks_visible_entry_visual_state() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.panes[ShellPaneId::SLOT_0].entries[0] =
            test_entry_with_mime_and_modified("beta.png", false, "image/png", Some(42));
        let renamed = ShellRenderDirtyKey::from_scene(&scene, size);

        assert_ne!(initial, renamed);
    }

    #[test]
    fn render_dirty_key_tracks_visible_details_entry_visual_state() {
        let mut scene = test_scene(
            (0..24)
                .map(|index| test_entry(&format!("file-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let visible_entry =
            projection.view.filtered_indexes[projection.visible_items[0].layout.model_index];
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.panes[ShellPaneId::SLOT_0].entries[visible_entry] =
            test_entry_with_mime_and_modified("visible-renamed.png", false, "image/png", Some(42));
        let renamed = ShellRenderDirtyKey::from_scene(&scene, size);

        assert_ne!(initial, renamed);
    }

    #[test]
    fn render_dirty_key_ignores_offscreen_details_entry_visual_state() {
        let mut scene = test_scene(
            (0..48)
                .map(|index| test_entry(&format!("file-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let visible_entry_indexes = projection
            .visible_items
            .iter()
            .filter_map(|item| {
                projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
            })
            .collect::<HashSet<_>>();
        let offscreen_entry = (0..scene.panes[ShellPaneId::SLOT_0].entries.len())
            .find(|index| !visible_entry_indexes.contains(index))
            .expect("test needs at least one offscreen details entry");
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.panes[ShellPaneId::SLOT_0].entries[offscreen_entry] =
            test_entry_with_mime_and_modified(
                "offscreen-renamed.png",
                false,
                "image/png",
                Some(42),
            );
        let offscreen_changed = ShellRenderDirtyKey::from_scene(&scene, size);

        assert_eq!(initial, offscreen_changed);
    }
