use super::model::AppState;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

pub fn load_state(path: impl AsRef<Path>) -> Result<AppState, StateStoreError> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(StateStoreError::Parse),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(AppState::default()),
        Err(err) => Err(StateStoreError::Read(err)),
    }
}

pub fn save_state(path: impl AsRef<Path>, state: &AppState) -> Result<(), StateStoreError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(StateStoreError::CreateDir)?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(StateStoreError::Serialize)?;
    fs::write(path, contents).map_err(StateStoreError::Write)
}

#[derive(Debug)]
pub enum StateStoreError {
    Read(io::Error),
    Parse(serde_json::Error),
    CreateDir(io::Error),
    Serialize(serde_json::Error),
    Write(io::Error),
}

impl fmt::Display for StateStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => write!(f, "failed to read state: {source}"),
            Self::Parse(source) => write!(f, "failed to parse state JSON: {source}"),
            Self::CreateDir(source) => write!(f, "failed to create state directory: {source}"),
            Self::Serialize(source) => write!(f, "failed to serialize state: {source}"),
            Self::Write(source) => write!(f, "failed to write state: {source}"),
        }
    }
}

impl std::error::Error for StateStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(source) | Self::CreateDir(source) | Self::Write(source) => Some(source),
            Self::Parse(source) | Self::Serialize(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn saves_and_loads_state() {
        let path = temp_path("state.json");
        let mut state = AppState::default();
        state.set_wallpaper(Some("eDP-1"), "demo.gwpdir");
        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();
        assert_eq!(loaded, state);
        let _ = fs::remove_file(path);
    }

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "gilder-state-test-{}-{nonce}-{name}",
            std::process::id()
        ))
    }
}
