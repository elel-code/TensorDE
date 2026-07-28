use serde::{Deserialize, Serialize};

/// Immutable metadata attached to clients accepted through a security-context listener.
///
/// This value deliberately excludes the listener and accepted socket. Wire adapters own
/// those descriptors while policy can retain and inspect this Smithay-free snapshot.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SecurityContextMetadata {
    pub sandbox_engine: Option<String>,
    pub app_id: Option<String>,
    pub instance_id: Option<String>,
}

impl SecurityContextMetadata {
    pub fn new(
        sandbox_engine: Option<String>,
        app_id: Option<String>,
        instance_id: Option<String>,
    ) -> Self {
        Self {
            sandbox_engine,
            app_id,
            instance_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trips_as_a_value() {
        let metadata = SecurityContextMetadata::new(
            Some("org.flatpak".to_owned()),
            Some("org.tensor.Test".to_owned()),
            Some("instance-7".to_owned()),
        );
        let json = serde_json::to_string(&metadata).unwrap();
        assert_eq!(
            serde_json::from_str::<SecurityContextMetadata>(&json).unwrap(),
            metadata
        );
    }
}
