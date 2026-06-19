use serde::{Deserialize, Serialize};

pub const FORMAT_NAME: &str = "gilder.wallpaper";
pub const FORMAT_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.gilder.json";
pub const MANIFEST_TOML_FILE: &str = "manifest.gilder.toml";
pub const PACKAGE_EXTENSION: &str = "gwp";
pub const DIRECTORY_EXTENSION: &str = "gwpdir";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperKind {
    StaticImage,
    Video,
    Slideshow,
    Web,
    SceneLite,
    Playlist,
}

impl WallpaperKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaticImage => "static-image",
            Self::Video => "video",
            Self::Slideshow => "slideshow",
            Self::Web => "web",
            Self::SceneLite => "scene-lite",
            Self::Playlist => "playlist",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_kind_names_are_stable() {
        assert_eq!(WallpaperKind::StaticImage.as_str(), "static-image");
        assert_eq!(WallpaperKind::Video.as_str(), "video");
        assert_eq!(WallpaperKind::Slideshow.as_str(), "slideshow");
        assert_eq!(WallpaperKind::Web.as_str(), "web");
        assert_eq!(WallpaperKind::SceneLite.as_str(), "scene-lite");
        assert_eq!(WallpaperKind::Playlist.as_str(), "playlist");
    }
}
