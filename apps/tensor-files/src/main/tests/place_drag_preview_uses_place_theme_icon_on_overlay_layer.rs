
    #[test]
    fn place_drag_preview_source_exposes_place_theme_icon_for_wayland_dnd() {
        // Drag preview is a compositor DnD icon, not an in-window overlay.
        // Verify the preview source still carries the place theme icon name
        // so `start_outgoing_drag` can render the same icon into a DragIcon.
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![ShellPlace::new(
            "",
            "H",
            "Home",
            PathBuf::from("/tmp"),
            false,
        )];
        let icon_name = scene.places[0].icon_name;
        let size = PhysicalSize::new(700, 360);
        let start = ViewPoint { x: 120.0, y: 120.0 };
        assert!(scene.begin_internal_drag_for_place(0, start));
        // Motion alone does not dirty the window (Wayland owns the DnD icon).
        let _ = scene.set_pointer(ViewPoint { x: 136.0, y: 136.0 }, size);

        let preview = scene
            .active_internal_drag_preview_source(size)
            .expect("active place drag should expose a preview source");
        let ShellInternalDragPreviewSource::Place {
            icon_name: preview_icon,
            layout,
            ..
        } = preview
        else {
            panic!("expected place preview source");
        };
        assert_eq!(preview_icon, icon_name);
        assert!(layout.bounds.width > 0.0);
        assert!(layout.bounds.height > 0.0);
        assert!(layout.icon.width > 0.0);
    }
