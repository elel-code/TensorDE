use super::*;

#[test]
fn parses_and_validates_static_manifest() {
    let json = r##"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.static",
          "version": "1.0.0",
          "title": "Example Static",
          "kind": "static-image",
          "preview": {
            "thumbnail": "previews/thumbnail.svg",
            "poster": "previews/poster.svg"
          },
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.svg",
            "fit": "cover",
            "background": "#000000"
          },
          "runtime": {
            "pause_when_fullscreen": true
          }
        }
        "##;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();
    assert_eq!(manifest.kind, WallpaperKind::StaticImage);
    assert_eq!(manifest.referenced_paths().unwrap().len(), 3);
}

#[test]
fn parses_and_validates_shader_manifest() {
    let json = r##"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.shader",
          "version": "1.0.0",
          "title": "Example Shader",
          "kind": "shader",
          "entry": {
            "type": "shader",
            "source": "shaders/main.frag",
            "fallback": "previews/poster.svg",
            "language": "glsl",
            "max_fps": 60,
            "uniforms": [
              { "name": "u_time", "source": "time" },
              { "name": "u_resolution", "source": "resolution" },
              { "name": "u_mouse", "source": "mouse" },
              { "name": "u_intensity", "source": "property", "property": "intensity" }
            ]
          },
          "properties": {
            "intensity": {
              "type": "range",
              "min": 0.0,
              "max": 1.0,
              "default": 0.5
            }
          }
        }
        "##;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();
    assert_eq!(manifest.kind, WallpaperKind::Shader);
    let WallpaperEntry::Shader {
        language, uniforms, ..
    } = &manifest.entry
    else {
        panic!("expected shader entry");
    };
    assert_eq!(*language, ShaderLanguage::Glsl);
    assert_eq!(uniforms.len(), 4);
    assert_eq!(manifest.referenced_paths().unwrap().len(), 2);
}

#[test]
fn rejects_invalid_shader_uniforms() {
    let json = r##"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-shader",
          "version": "1.0.0",
          "title": "Bad Shader",
          "kind": "shader",
          "entry": {
            "type": "shader",
            "source": "shaders/main.wgsl",
            "uniforms": [
              { "name": "u_value", "source": "property" }
            ]
          }
        }
        "##;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn rejects_duplicate_shader_uniform_names() {
    let json = r##"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.duplicate-shader-uniform",
          "version": "1.0.0",
          "title": "Duplicate Shader Uniform",
          "kind": "shader",
          "entry": {
            "type": "shader",
            "source": "shaders/main.frag",
            "fallback": "previews/poster.svg",
            "uniforms": [
              { "name": "u_time", "source": "time" },
              { "name": "u_time", "source": "resolution" }
            ]
          }
        }
        "##;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn rejects_kind_mismatch() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.mismatch",
          "version": "1.0.0",
          "title": "Mismatch",
          "kind": "video",
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.png"
          }
        }
        "#;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::KindMismatch { .. })
    ));
}

#[test]
fn rejects_invalid_static_entry_dimensions() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-dimensions",
          "version": "1.0.0",
          "title": "Bad Dimensions",
          "kind": "static-image",
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.png",
            "width": 0,
            "height": 1080
          }
        }
        "#;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn parses_and_validates_playlist_manifest() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.playlist",
          "version": "1.0.0",
          "title": "Playlist",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": [
              {
                "id": "battery-static",
                "conditions": { "power": "battery" },
                "entry": {
                  "type": "static-image",
                  "source": "assets/battery.png"
                }
              },
              {
                "id": "default-video",
                "entry": {
                  "type": "video",
                  "source": "assets/loop.webm"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();

    assert_eq!(manifest.kind, WallpaperKind::Playlist);
    let WallpaperEntry::Playlist { items, selection } = &manifest.entry else {
        panic!("expected playlist entry");
    };
    assert_eq!(*selection, PlaylistSelection::FirstMatch);
    assert_eq!(items[0].weight, 1);
    assert_eq!(manifest.referenced_paths().unwrap().len(), 2);
}

#[test]
fn parses_and_validates_weighted_playlist_manifest() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.weighted-playlist",
          "version": "1.0.0",
          "title": "Weighted Playlist",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
              {
                "id": "rare",
                "weight": 1,
                "entry": {
                  "type": "static-image",
                  "source": "assets/rare.png"
                }
              },
              {
                "id": "common",
                "weight": 9,
                "entry": {
                  "type": "static-image",
                  "source": "assets/common.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();
    let WallpaperEntry::Playlist { items, selection } = &manifest.entry else {
        panic!("expected playlist entry");
    };
    assert_eq!(*selection, PlaylistSelection::WeightedRandom);
    assert_eq!(items[0].weight, 1);
    assert_eq!(items[1].weight, 9);
}

#[test]
fn rejects_zero_weight_playlist_item() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-playlist-weight",
          "version": "1.0.0",
          "title": "Bad Playlist Weight",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
              {
                "id": "disabled",
                "weight": 0,
                "entry": {
                  "type": "static-image",
                  "source": "assets/disabled.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn parses_and_validates_playlist_local_time_condition() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.playlist-time",
          "version": "1.0.0",
          "title": "Playlist Time",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": [
              {
                "id": "day",
                "conditions": {
                  "local_time": {
                    "start": "08:30",
                    "end": "18:00"
                  }
                },
                "entry": {
                  "type": "static-image",
                  "source": "assets/day.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();
    let WallpaperEntry::Playlist { items, .. } = &manifest.entry else {
        panic!("expected playlist entry");
    };
    let local_time = items[0].conditions.local_time.as_ref().unwrap();
    assert!(local_time.contains_minute_of_day(9 * 60));
    assert!(!local_time.contains_minute_of_day(18 * 60));
}

#[test]
fn parses_and_validates_playlist_weekday_condition() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.playlist-weekday",
          "version": "1.0.0",
          "title": "Playlist Weekday",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": [
              {
                "id": "workday",
                "conditions": {
                  "weekdays": ["monday", "tue", "friday"]
                },
                "entry": {
                  "type": "static-image",
                  "source": "assets/workday.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    manifest.validate().unwrap();
    let WallpaperEntry::Playlist { items, .. } = &manifest.entry else {
        panic!("expected playlist entry");
    };
    assert_eq!(
        items[0].conditions.weekdays,
        vec![
            PlaylistWeekday::Monday,
            PlaylistWeekday::Tuesday,
            PlaylistWeekday::Friday
        ]
    );
}

#[test]
fn rejects_duplicate_playlist_weekday_condition() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-playlist-weekday",
          "version": "1.0.0",
          "title": "Bad Playlist Weekday",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": [
              {
                "id": "duplicate",
                "conditions": {
                  "weekdays": ["monday", "mon"]
                },
                "entry": {
                  "type": "static-image",
                  "source": "assets/day.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn rejects_invalid_playlist_local_time_condition() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-playlist-time",
          "version": "1.0.0",
          "title": "Bad Playlist Time",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": [
              {
                "id": "bad",
                "conditions": {
                  "local_time": {
                    "start": "24:00",
                    "end": "18:00"
                  }
                },
                "entry": {
                  "type": "static-image",
                  "source": "assets/day.png"
                }
              }
            ]
          }
        }
        "#;

    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn rejects_empty_playlist_manifest() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.empty-playlist",
          "version": "1.0.0",
          "title": "Empty Playlist",
          "kind": "playlist",
          "entry": {
            "type": "playlist",
            "items": []
          }
        }
        "#;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidEntry(_))
    ));
}

#[test]
fn rejects_invalid_property_schema() {
    let json = r#"
        {
          "format": "tensor-wallpaper.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-property",
          "version": "1.0.0",
          "title": "Bad Property",
          "kind": "static-image",
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.png"
          },
          "properties": {
            "fit": {
              "type": "choice",
              "default": "cover",
              "choices": []
            }
          }
        }
        "#;
    let manifest: Manifest = serde_json::from_str(json).unwrap();
    assert!(matches!(
        manifest.validate(),
        Err(ManifestError::InvalidProperty { .. })
    ));
}
