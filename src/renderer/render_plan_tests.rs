    use super::*;
    use crate::config::{
        DynamicPausePolicy, GilderConfig, OutputConfig, OutputPerformanceConfig, PerformanceConfig,
        PowerPolicy, VideoDecoderPolicy,
    };
    use crate::core::pack_gwp;
    use crate::desktop::{DesktopCursorParallax, DesktopOutput, PowerState};
    use crate::policy::{DecisionReason, PerformanceDecision, RenderMode};
    use crate::state::{OutputState, WallpaperAssignment};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn playlist_test_clock(
        local_minute_of_day: u16,
        local_weekday: PlaylistWeekday,
    ) -> PlaylistClockKey {
        PlaylistClockKey {
            local_minute_of_day,
            local_weekday,
        }
    }

    #[test]
    fn builds_static_wallpaper_plan_from_package() {
        let package = crate::core::load_gwpdir("examples/wallpapers/static-demo.gwpdir").unwrap();
        let output_state = OutputState {
            wallpaper: Some(WallpaperAssignment {
                path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
                variant: None,
            }),
            ..OutputState::default()
        };

        let plan = static_wallpaper_plan("eDP-1", &package, &output_state)
            .unwrap()
            .unwrap();
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.fit, FitMode::Cover);
        assert_eq!(plan.background.as_deref(), Some("#101418"));
        assert!(plan.source.ends_with("assets/wallpaper.svg"));
    }

    #[test]
    fn builds_slideshow_plan_from_example_package() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/slideshow-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert!(sync.errors.is_empty());
        let plan = &sync.slideshow_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.sources.len(), 2);
        assert!(plan.sources[0].ends_with("assets/slide-a.svg"));
        assert!(plan.sources[1].ends_with("assets/slide-b.svg"));
        assert_eq!(plan.interval_ms, 3_000);
        assert_eq!(plan.fit, FitMode::Cover);
    }

    #[test]
    fn playlist_selects_wallpaper_from_power_condition() {
        let test_dir = TestDir::new("gilder-playlist-power-plan");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_minimal_playlist_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });

        let battery_sync = static_render_sync_plan(
            &DesktopSnapshot {
                power: PowerState::Battery,
                outputs: vec![DesktopOutput::virtual_output("eDP-1")],
                ..DesktopSnapshot::default()
            },
            &state,
            test_dir.path.join("cache"),
        );

        assert!(battery_sync.errors.is_empty());
        assert_eq!(battery_sync.plans.len(), 1);
        assert!(battery_sync.video_plans.is_empty());
        assert!(battery_sync.plans[0].source.ends_with("assets/battery.svg"));

        let ac_sync = static_render_sync_plan(
            &DesktopSnapshot {
                power: PowerState::Ac,
                outputs: vec![DesktopOutput::virtual_output("eDP-1")],
                ..DesktopSnapshot::default()
            },
            &state,
            test_dir.path.join("cache"),
        );

        assert!(ac_sync.errors.is_empty());
        assert!(ac_sync.plans.is_empty());
        assert_eq!(ac_sync.video_plans.len(), 1);
        assert!(ac_sync.video_plans[0].source.ends_with("assets/loop.webm"));
    }

    #[test]
    fn playlist_selects_wallpaper_from_local_time_condition() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "day",
                    "conditions": {
                        "local_time": {
                            "start": "08:00",
                            "end": "18:00"
                        }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/day.svg"
                    }
                },
                {
                    "id": "night",
                    "conditions": {
                        "local_time": {
                            "start": "18:00",
                            "end": "08:00"
                        }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/night.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, .. } = &entry else {
            panic!("expected playlist entry");
        };
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");

        let day_context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(10 * 60 + 30, PlaylistWeekday::Monday),
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&day_context))
                .map(|item| item.id.as_str()),
            Some("day")
        );

        let night_context = PlaylistRenderContext {
            local_clock: playlist_test_clock(22 * 60 + 30, PlaylistWeekday::Monday),
            ..day_context
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&night_context))
                .map(|item| item.id.as_str()),
            Some("night")
        );
    }

    #[test]
    fn playlist_selects_wallpaper_from_weekday_condition() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "workday",
                    "conditions": {
                        "weekdays": ["monday", "tuesday", "wednesday", "thursday", "friday"]
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/workday.svg"
                    }
                },
                {
                    "id": "weekend",
                    "conditions": {
                        "weekdays": ["sat", "sun"]
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weekend.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, .. } = &entry else {
            panic!("expected playlist entry");
        };
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");
        let monday_context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(10 * 60, PlaylistWeekday::Monday),
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&monday_context))
                .map(|item| item.id.as_str()),
            Some("workday")
        );

        let sunday_context = PlaylistRenderContext {
            local_clock: playlist_test_clock(10 * 60, PlaylistWeekday::Sunday),
            ..monday_context
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&sunday_context))
                .map(|item| item.id.as_str()),
            Some("weekend")
        );
    }

    #[test]
    fn computes_gregorian_weekdays_for_playlist_clock() {
        assert_eq!(gregorian_weekday(2026, 6, 19), PlaylistWeekday::Friday);
        assert_eq!(gregorian_weekday(2024, 2, 29), PlaylistWeekday::Thursday);
        assert_eq!(gregorian_weekday(1970, 1, 1), PlaylistWeekday::Thursday);
    }

    #[test]
    fn playlist_weighted_random_selection_is_stable_and_weighted() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
                {
                    "id": "rare",
                    "weight": 1,
                    "entry": {
                        "type": "static-image",
                        "source": "assets/rare.svg"
                    }
                },
                {
                    "id": "common",
                    "weight": 9,
                    "entry": {
                        "type": "static-image",
                        "source": "assets/common.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, selection } = &entry else {
            panic!("expected playlist entry");
        };
        assert_eq!(*selection, PlaylistSelection::WeightedRandom);

        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");
        let context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(11 * 60 + 7, PlaylistWeekday::Monday),
        };
        let first =
            select_playlist_item(items, *selection, Some(&context)).map(|item| item.id.as_str());
        let second =
            select_playlist_item(items, *selection, Some(&context)).map(|item| item.id.as_str());
        assert_eq!(first, second);

        let mut rare_count = 0;
        let mut common_count = 0;
        for local_minute_of_day in 0..(24 * 60) {
            let context = PlaylistRenderContext {
                local_clock: playlist_test_clock(local_minute_of_day, PlaylistWeekday::Monday),
                ..context
            };
            match select_playlist_item(items, *selection, Some(&context))
                .map(|item| item.id.as_str())
            {
                Some("rare") => rare_count += 1,
                Some("common") => common_count += 1,
                other => panic!("unexpected weighted playlist item {other:?}"),
            }
        }

        assert!(common_count > rare_count * 3);
    }

    #[test]
    fn playlist_clock_dependency_tracks_time_sensitive_selection() {
        let power_only: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "battery",
                    "conditions": { "power": "battery" },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/battery.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&power_only),
            PlaylistClockDependency::None
        );

        let local_time: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "day",
                    "conditions": {
                        "local_time": { "start": "08:00", "end": "18:00" }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/day.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&local_time),
            PlaylistClockDependency::Minute
        );

        let weekdays: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "weekday",
                    "conditions": { "weekdays": ["monday"] },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weekday.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&weekdays),
            PlaylistClockDependency::Weekday
        );

        let weighted: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
                {
                    "id": "weighted",
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weighted.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&weighted),
            PlaylistClockDependency::MinuteAndWeekday
        );
    }

    #[test]
    fn playlist_clock_cache_key_uses_only_required_fields() {
        let clock = playlist_test_clock(11 * 60 + 7, PlaylistWeekday::Friday);

        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::None, clock),
            None
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::Minute, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: Some(11 * 60 + 7),
                local_weekday: None,
            })
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::Weekday, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: None,
                local_weekday: Some(PlaylistWeekday::Friday),
            })
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::MinuteAndWeekday, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: Some(11 * 60 + 7),
                local_weekday: Some(PlaylistWeekday::Friday),
            })
        );
    }

    #[test]
    fn playlist_static_selection_survives_battery_pause_dynamic_policy() {
        let test_dir = TestDir::new("gilder-playlist-battery-static");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_minimal_playlist_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/battery.svg"));
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
    }

    #[test]
    fn playlist_no_match_reports_error_under_pause_dynamic_policy() {
        let test_dir = TestDir::new("gilder-playlist-no-match");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_playlist_no_match_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.removals.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(sync.errors[0].message, "playlist did not match any item");
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn static_wallpaper_plan_uses_requested_variant_source() {
        let test_dir = TestDir::new("gilder-static-variant-plan");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("wide".to_owned()),
        };

        let plan =
            static_wallpaper_plan_for_assignment("eDP-1", &assignment, test_dir.path.join("cache"))
                .unwrap();

        assert!(plan.source.ends_with("assets/wide.svg"));
    }

    #[test]
    fn missing_requested_variant_reports_error() {
        let test_dir = TestDir::new("gilder-missing-variant-plan");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("missing".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(
            sync.errors[0].message,
            "wallpaper variant \"missing\" was not found"
        );
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn auto_selects_smallest_variant_covering_scaled_output() {
        let test_dir = TestDir::new("gilder-auto-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(960),
                height: Some(540),
                scale: 2.0,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/hd.svg"));
    }

    #[test]
    fn explicit_variant_overrides_automatic_variant_selection() {
        let test_dir = TestDir::new("gilder-explicit-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("uhd".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(1920),
                height: Some(1080),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/uhd.svg"));
    }

    #[test]
    fn automatic_variant_keeps_entry_source_when_no_variant_covers_output() {
        let test_dir = TestDir::new("gilder-no-cover-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(5000),
                height: Some(3000),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/wallpaper.svg"));
    }

    #[test]
    fn runtime_static_image_cache_generates_and_reuses_downscaled_source() {
        let test_dir = TestDir::new("gilder-static-runtime-cache");
        let package_dir = test_dir.path.join("static-large.gwpdir");
        let cache_dir = test_dir.path.join("cache");
        let ffmpeg = test_dir.path.join("ffmpeg");
        write_static_large_gwpdir(&package_dir);
        write_executable_script(
            &ffmpeg,
            r#"#!/bin/sh
out=""
for arg in "$@"; do
  out="$arg"
done
printf 'cached-static' > "$out"
exit 0
"#,
        );
        let package = crate::core::load_gwpdir(&package_dir).unwrap();
        let performance = active_performance_decision();
        let mut stats = RenderSyncCacheReport::default();
        let mut protected = BTreeSet::new();

        let first_source = {
            let mut context = StaticImageCacheContext {
                cache_dir: &cache_dir,
                max_entries: 8,
                stats: &mut stats,
                protected_files: &mut protected,
                ffmpeg: Some(&ffmpeg),
            };
            let plan = wallpaper_plan_with_target(
                "eDP-1",
                &package,
                &performance,
                VideoDecoderPolicy::default(),
                None,
                None,
                Some(RenderTargetSize {
                    width: 1920,
                    height: 1080,
                }),
                None,
                None,
                false,
                Some(&mut context),
            )
            .unwrap();
            match plan {
                WallpaperRenderPlan::StaticImage(plan) => plan.source,
                _ => panic!("expected static image plan"),
            }
        };

        assert!(first_source.starts_with(cache_dir.join("static-image-cache")));
        assert_eq!(fs::read(&first_source).unwrap(), b"cached-static");
        assert_eq!(stats.static_image_cache_generations, 1);
        assert_eq!(stats.static_image_cache_reuses, 0);
        assert_eq!(stats.static_image_cache_generation_errors, 0);

        let second_source = {
            let mut context = StaticImageCacheContext {
                cache_dir: &cache_dir,
                max_entries: 8,
                stats: &mut stats,
                protected_files: &mut protected,
                ffmpeg: Some(&ffmpeg),
            };
            let plan = wallpaper_plan_with_target(
                "eDP-1",
                &package,
                &performance,
                VideoDecoderPolicy::default(),
                None,
                None,
                Some(RenderTargetSize {
                    width: 1920,
                    height: 1080,
                }),
                None,
                None,
                false,
                Some(&mut context),
            )
            .unwrap();
            match plan {
                WallpaperRenderPlan::StaticImage(plan) => plan.source,
                _ => panic!("expected static image plan"),
            }
        };

        assert_eq!(second_source, first_source);
        assert_eq!(stats.static_image_cache_generations, 1);
        assert_eq!(stats.static_image_cache_reuses, 1);
        assert!(protected.contains(&first_source));
    }

    #[test]
    fn static_image_cache_accepts_tall_contain_sources() {
        assert!(should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 1200,
                height: 8000,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Contain,
        ));
    }

    #[test]
    fn static_image_cache_does_not_upscale_small_contain_sources() {
        assert!(!should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 800,
                height: 600,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Contain,
        ));
    }

    #[test]
    fn static_image_cache_accepts_stretch_when_area_shrinks() {
        assert!(should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 9000,
                height: 500,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Stretch,
        ));
    }

    #[test]
    fn skips_output_without_wallpaper_assignment() {
        let package = crate::core::load_gwpdir("examples/wallpapers/static-demo.gwpdir").unwrap();
        let plan = static_wallpaper_plan("eDP-1", &package, &OutputState::default()).unwrap();
        assert_eq!(plan, None);
    }

    #[test]
    fn builds_sync_plan_for_default_and_per_output_wallpapers() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        state.outputs.insert(
            "DP-1".to_owned(),
            OutputState {
                wallpaper: Some(WallpaperAssignment {
                    path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
                    variant: None,
                }),
                ..OutputState::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![crate::desktop::DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, std::env::temp_dir());
        assert_eq!(sync.plans.len(), 2);
        assert!(sync.errors.is_empty());
        assert!(sync.plans.iter().any(|plan| plan.output_name == "eDP-1"));
        assert!(sync.plans.iter().any(|plan| plan.output_name == "DP-1"));
        assert_eq!(sync.decisions.len(), 2);
        assert!(
            sync.decisions
                .iter()
                .all(|decision| decision.action == StaticRenderAction::Render)
        );
        assert!(sync.video_plans.is_empty());
    }

    #[test]
    fn config_default_wallpaper_builds_plan_for_desktop_output() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "eDP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn adaptive_snapshot_throttles_render_sync_decision() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.throttle_max_fps = 15;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_unfocused_removes_unfocused_output_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseUnfocused;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn battery_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Battery
        );
    }

    #[test]
    fn battery_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn hidden_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::OutputHidden
        );
    }

    #[test]
    fn hidden_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn session_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_active: false,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::SessionInactive
        );
    }

    #[test]
    fn session_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_locked: true,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn unfocused_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn unfocused_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn config_output_wallpaper_adds_named_output_without_state() {
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "DP-1".to_owned(),
            OutputConfig {
                wallpaper: Some("examples/wallpapers/static-demo.gwpdir".to_owned()),
                ..OutputConfig::default()
            },
        );
        let state = AppState::default();
        let desktop = DesktopSnapshot::default();

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "DP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn persisted_state_wallpaper_overrides_config_default() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("missing-config-default.gwpdir".to_owned());
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_policy_removes_output_without_loading_wallpaper() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "missing-wallpaper.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("missing-wallpaper.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_removes_slideshow_after_manifest_load() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_keeps_static_wallpaper_renderable() {
        let test_dir = TestDir::new("gilder-fullscreen-pause-dynamic-static");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_continue_when_fullscreen(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn throttled_policy_keeps_static_plan_with_decision() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn manifest_runtime_policy_can_pause_unfocused_output() {
        let test_dir = TestDir::new("gilder-runtime-unfocused-pause");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_pause_when_unfocused(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn builds_video_sync_plan_with_effective_fps() {
        let test_dir = TestDir::new("gilder-video-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = PerformanceConfig::default();
        config.background_max_fps = 15;
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        let plan = &sync.video_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/loop.webm"));
        assert!(
            plan.poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
        assert_eq!(plan.fit, FitMode::Contain);
        assert!(!plan.loop_playback);
        assert!(plan.muted);
        assert_eq!(plan.manifest_max_fps, Some(60));
        assert_eq!(plan.target_max_fps, Some(15));
        assert_eq!(plan.start_offset_ms, 1200);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
    }

    #[test]
    fn video_plan_keeps_audio_unmuted_when_runtime_allows_audio() {
        let test_dir = TestDir::new("gilder-video-runtime-audio");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        set_runtime_allow_audio(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(!sync.video_plans[0].muted);
    }

    #[test]
    fn output_performance_override_sets_video_target_fps() {
        let test_dir = TestDir::new("gilder-output-performance-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.video.decoder = VideoDecoderPolicy::Software;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                performance: OutputPerformanceConfig {
                    background_max_fps: Some(12),
                    ..OutputPerformanceConfig::default()
                },
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].target_max_fps, Some(12));
        assert_eq!(
            sync.video_plans[0].decoder_policy,
            VideoDecoderPolicy::Software
        );
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(12));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn output_fit_override_sets_video_and_poster_fit() {
        let test_dir = TestDir::new("gilder-output-fit-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                fit: Some(FitMode::Stretch),
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].fit, FitMode::Stretch);
    }

    #[test]
    fn video_plan_uses_requested_variant_source() {
        let test_dir = TestDir::new("gilder-video-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("mobile".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some(package_dir.display().to_string().as_str())
        );
    }

    #[test]
    fn video_plan_auto_selects_portrait_variant_source() {
        let test_dir = TestDir::new("gilder-video-auto-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(1080),
                height: Some(1920),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
    }

    #[test]
    fn video_plan_uses_preview_poster_when_entry_poster_is_missing() {
        let test_dir = TestDir::new("gilder-video-preview-poster");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        remove_entry_poster(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.plans.is_empty());
        assert!(
            sync.video_plans[0]
                .poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
    }
    #[test]
    fn web_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-web-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn web_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("gilder-web-without-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        remove_entry_fallback(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(sync.errors[0].message, "web entries are not supported here");
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn shader_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-shader-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn shader_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("gilder-shader-without-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        remove_entry_fallback(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(
            sync.errors[0].message,
            "shader entries are not supported here"
        );
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn pause_dynamic_releases_shader_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-shader-pause-dynamic");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }

    #[test]
    fn pause_dynamic_releases_web_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-web-pause-dynamic");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }
