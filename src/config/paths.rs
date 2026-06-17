use std::env;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationPaths {
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl ApplicationPaths {
    pub fn from_env() -> Result<Self, PathError> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(PathError::MissingHome)?;

        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let state_home = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"));
        let cache_home = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));

        Ok(Self {
            config_file: config_home.join("gilder/config.toml"),
            state_file: state_home.join("gilder/state.json"),
            cache_dir: cache_home.join("gilder"),
            data_dir: data_home.join("gilder"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    MissingHome,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHome => f.write_str("HOME is not set; cannot resolve XDG paths"),
        }
    }
}

impl std::error::Error for PathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_xdg_paths_from_home() {
        let home = PathBuf::from("/home/example");
        let config_home = home.join(".config");
        let state_home = home.join(".local/state");
        assert_eq!(
            config_home.join("gilder/config.toml"),
            home.join(".config/gilder/config.toml")
        );
        assert_eq!(
            state_home.join("gilder/state.json"),
            home.join(".local/state/gilder/state.json")
        );
    }
}
