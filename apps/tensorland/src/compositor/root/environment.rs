//! Session environment policy at the compositor ownership boundary.

use std::{collections::BTreeSet, ffi::OsString};

use tracing::warn;

use crate::{config::EnvironmentConfig, service::EnvironmentValue};

/// Merge user KDL policy without allowing it to replace session-owned names.
pub(super) fn apply_user_environment(
    environment: &mut Vec<EnvironmentValue>,
    policy: &EnvironmentConfig,
) {
    let session_names: BTreeSet<OsString> = crate::service::SESSION_ENVIRONMENT_NAMES
        .iter()
        .map(|name| OsString::from(*name))
        .collect();
    for name in &policy.clear {
        let key = OsString::from(name);
        if session_names.contains(&key) {
            continue;
        }
        environment.retain(|(existing, _)| existing != &key);
    }
    for (name, value) in &policy.set {
        let key = OsString::from(name);
        if session_names.contains(&key) {
            warn!(
                name,
                "ignoring environment/set for a session-owned variable"
            );
            continue;
        }
        if let Some(entry) = environment
            .iter_mut()
            .find(|(existing, _)| existing == &key)
        {
            entry.1 = OsString::from(value);
        } else {
            environment.push((key, OsString::from(value)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::session_environment;

    #[test]
    fn user_environment_cannot_override_session_names() {
        let mut environment =
            session_environment("wayland-1", "/tmp/tensor.sock", None, "default", 24);
        apply_user_environment(
            &mut environment,
            &EnvironmentConfig {
                clear: vec![
                    "WAYLAND_DISPLAY".to_owned(),
                    "XCURSOR_THEME".to_owned(),
                    "EDITOR".to_owned(),
                ],
                set: [
                    ("WAYLAND_DISPLAY".to_owned(), "evil".to_owned()),
                    ("XCURSOR_THEME".to_owned(), "evil".to_owned()),
                    ("EDITOR".to_owned(), "hx".to_owned()),
                ]
                .into_iter()
                .collect(),
            },
        );
        assert_eq!(
            environment
                .iter()
                .find(|(name, _)| name == "XCURSOR_THEME")
                .map(|(_, value)| value.as_os_str()),
            Some(std::ffi::OsStr::new("default"))
        );
        assert_eq!(
            environment
                .iter()
                .find(|(name, _)| name == "WAYLAND_DISPLAY")
                .map(|(_, value)| value.as_os_str()),
            Some(std::ffi::OsStr::new("wayland-1"))
        );
        assert_eq!(
            environment
                .iter()
                .find(|(name, _)| name == "EDITOR")
                .map(|(_, value)| value.as_os_str()),
            Some(std::ffi::OsStr::new("hx"))
        );
    }
}
