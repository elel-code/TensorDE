//! Rendering plans and optional GTK/layer-shell renderer.

#[cfg(feature = "gtk-renderer")]
pub mod gtk;

use crate::core::{FitMode, WallpaperEntry, WallpaperPackage};
use crate::desktop::DesktopSnapshot;
use crate::state::{AppState, OutputState, WallpaperAssignment};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticWallpaperPlan {
    pub output_name: String,
    pub source: PathBuf,
    pub fit: FitMode,
    pub background: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRenderSyncPlan {
    pub plans: Vec<StaticWallpaperPlan>,
    pub removals: Vec<String>,
    pub errors: Vec<StaticRenderPlanFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRenderPlanFailure {
    pub output_name: String,
    pub wallpaper: String,
    pub message: String,
}

pub fn static_render_sync_plan(
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: impl AsRef<Path>,
) -> StaticRenderSyncPlan {
    let mut output_names: Vec<String> = desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(state.outputs.keys().cloned())
        .collect();
    output_names.sort();
    output_names.dedup();

    let cache_dir = cache_dir.as_ref();
    let mut plans = Vec::new();
    let mut removals = Vec::new();
    let mut errors = Vec::new();
    for output_name in output_names {
        let output_state = state.outputs.get(&output_name).cloned().unwrap_or_default();
        if output_state.paused {
            removals.push(output_name);
            continue;
        }
        let assignment = output_state
            .wallpaper
            .as_ref()
            .or(state.default_wallpaper.as_ref());
        let Some(assignment) = assignment else {
            removals.push(output_name);
            continue;
        };
        match static_wallpaper_plan_for_assignment(&output_name, assignment, cache_dir) {
            Ok(plan) => plans.push(plan),
            Err(err) => errors.push(StaticRenderPlanFailure {
                output_name,
                wallpaper: assignment.path.clone(),
                message: err.to_string(),
            }),
        }
    }

    StaticRenderSyncPlan {
        plans,
        removals,
        errors,
    }
}

pub fn static_wallpaper_plan_for_assignment(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
) -> Result<StaticWallpaperPlan, RendererPlanError> {
    let package = load_assigned_package(assignment, cache_dir.as_ref())?;
    let output_state = OutputState {
        wallpaper: Some(assignment.clone()),
        ..OutputState::default()
    };
    static_wallpaper_plan(output_name, &package, &output_state)?
        .ok_or(RendererPlanError::MissingAssignment)
}

pub fn static_wallpaper_plan(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    output_state: &OutputState,
) -> Result<Option<StaticWallpaperPlan>, RendererPlanError> {
    let Some(_assignment) = &output_state.wallpaper else {
        return Ok(None);
    };
    let WallpaperEntry::StaticImage {
        source,
        fit,
        background,
        ..
    } = &package.manifest.entry
    else {
        return Err(RendererPlanError::UnsupportedEntry(
            package.manifest.entry.kind().as_str(),
        ));
    };

    Ok(Some(StaticWallpaperPlan {
        output_name: output_name.into(),
        source: source.join_to(&package.root),
        fit: *fit,
        background: background.clone(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererPlanError {
    UnsupportedEntry(&'static str),
    MissingAssignment,
    PackageLoad(String),
}

impl fmt::Display for RendererPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEntry(kind) => write!(f, "{kind} entries are not supported here"),
            Self::MissingAssignment => f.write_str("wallpaper assignment is missing"),
            Self::PackageLoad(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RendererPlanError {}

fn load_assigned_package(
    assignment: &WallpaperAssignment,
    cache_dir: &Path,
) -> Result<WallpaperPackage, RendererPlanError> {
    let path = Path::new(&assignment.path);
    if path.is_dir() || path.extension().and_then(|extension| extension.to_str()) == Some("gwpdir")
    {
        return crate::core::load_gwpdir(path)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()));
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("gwp") {
        let extract_dir = archive_extract_dir(cache_dir, path);
        if extract_dir.join(crate::core::MANIFEST_FILE).exists() {
            return crate::core::load_gwpdir(&extract_dir)
                .map_err(|err| RendererPlanError::PackageLoad(err.to_string()));
        }
        fs::create_dir_all(
            extract_dir
                .parent()
                .ok_or_else(|| RendererPlanError::PackageLoad("invalid cache path".to_owned()))?,
        )
        .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
        crate::core::load_gwp(path, &extract_dir)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))
    } else {
        Err(RendererPlanError::PackageLoad(format!(
            "unsupported wallpaper path {}",
            path.display()
        )))
    }
}

fn archive_extract_dir(cache_dir: &Path, archive_path: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    archive_path.hash(&mut hasher);
    let file_name = archive_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("wallpaper")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    cache_dir
        .join("render-cache")
        .join(format!("{}-{:016x}.gwpdir", file_name, hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pack_gwp;
    use crate::state::{OutputState, WallpaperAssignment};
    use std::time::{SystemTime, UNIX_EPOCH};

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
    }

    #[test]
    fn builds_plan_from_gwp_archive() {
        let test_dir = TestDir::new("gilder-render-archive");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache = test_dir.path.join("cache");
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let assignment = WallpaperAssignment {
            path: archive.display().to_string(),
            variant: None,
        };

        let plan = static_wallpaper_plan_for_assignment("eDP-1", &assignment, &cache).unwrap();
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/wallpaper.svg"));
        assert!(cache.join("render-cache").exists());
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
