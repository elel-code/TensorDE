use std::collections::HashMap;

use tensor_dbus::zvariant::OwnedValue;
use thiserror::Error;

use crate::AppearanceSettings;

pub const SETTINGS_INTERFACE: &str = "org.freedesktop.impl.portal.Settings";
pub const APPEARANCE_NAMESPACE: &str = "org.freedesktop.appearance";
pub const SETTINGS_VERSION: u32 = 1;
const MAX_NAMESPACE_FILTERS: usize = 64;
const MAX_NAMESPACE_BYTES: usize = 256;

pub type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsSnapshot {
    appearance: AppearanceSettings,
}

impl SettingsSnapshot {
    pub const fn new(appearance: AppearanceSettings) -> Self {
        Self { appearance }
    }

    pub fn read_all(&self, namespaces: &[String]) -> Result<SettingsMap, SettingsError> {
        validate_filters(namespaces)?;
        let mut result = HashMap::new();
        if namespaces_match(namespaces, APPEARANCE_NAMESPACE) {
            result.insert(APPEARANCE_NAMESPACE.to_owned(), self.appearance_values());
        }
        Ok(result)
    }

    pub fn read(&self, namespace: &str, key: &str) -> Result<OwnedValue, SettingsError> {
        if namespace != APPEARANCE_NAMESPACE {
            return Err(SettingsError::NotFound);
        }
        match key {
            "color-scheme" => Ok(self.appearance.color_scheme.portal_value().into()),
            "contrast" => Ok(self.appearance.contrast.portal_value().into()),
            "reduced-motion" => Ok(u32::from(self.appearance.reduced_motion).into()),
            _ => Err(SettingsError::NotFound),
        }
    }

    fn appearance_values(&self) -> HashMap<String, OwnedValue> {
        HashMap::from([
            (
                "color-scheme".to_owned(),
                self.appearance.color_scheme.portal_value().into(),
            ),
            (
                "contrast".to_owned(),
                self.appearance.contrast.portal_value().into(),
            ),
            (
                "reduced-motion".to_owned(),
                u32::from(self.appearance.reduced_motion).into(),
            ),
        ])
    }
}

fn validate_filters(filters: &[String]) -> Result<(), SettingsError> {
    if filters.len() > MAX_NAMESPACE_FILTERS
        || filters
            .iter()
            .any(|filter| filter.len() > MAX_NAMESPACE_BYTES)
    {
        return Err(SettingsError::InvalidFilters);
    }
    Ok(())
}

fn namespaces_match(filters: &[String], namespace: &str) -> bool {
    filters.is_empty()
        || filters.iter().any(|filter| {
            filter.is_empty()
                || filter == namespace
                || filter
                    .strip_suffix('*')
                    .is_some_and(|prefix| !prefix.contains('*') && namespace.starts_with(prefix))
        })
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SettingsError {
    #[error("unknown settings namespace or key")]
    NotFound,
    #[error("settings namespace filter list exceeds its bounded request limits")]
    InvalidFilters,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorScheme, Contrast};

    fn snapshot() -> SettingsSnapshot {
        SettingsSnapshot::new(AppearanceSettings {
            color_scheme: ColorScheme::Dark,
            contrast: Contrast::High,
            reduced_motion: true,
        })
    }

    #[test]
    fn read_all_supports_exact_trailing_glob_and_all_filters() {
        for filters in [
            vec![],
            vec![String::new()],
            vec![APPEARANCE_NAMESPACE.to_owned()],
            vec!["org.freedesktop.*".to_owned()],
        ] {
            let values = snapshot().read_all(&filters).unwrap();
            assert!(values.contains_key(APPEARANCE_NAMESPACE));
        }
        assert!(
            snapshot()
                .read_all(&["org.example.*".to_owned()])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn standardized_values_use_the_xdp_numeric_contract() {
        let snapshot = snapshot();
        assert_eq!(
            u32::try_from(snapshot.read(APPEARANCE_NAMESPACE, "color-scheme").unwrap()).unwrap(),
            1
        );
        assert_eq!(
            u32::try_from(snapshot.read(APPEARANCE_NAMESPACE, "contrast").unwrap()).unwrap(),
            1
        );
        assert_eq!(
            u32::try_from(
                snapshot
                    .read(APPEARANCE_NAMESPACE, "reduced-motion")
                    .unwrap()
            )
            .unwrap(),
            1
        );
        assert_eq!(
            snapshot.read(APPEARANCE_NAMESPACE, "missing"),
            Err(SettingsError::NotFound)
        );
    }

    #[test]
    fn namespace_filters_are_bounded_before_result_allocation() {
        let filters = vec![String::new(); MAX_NAMESPACE_FILTERS + 1];
        assert_eq!(
            snapshot().read_all(&filters),
            Err(SettingsError::InvalidFilters)
        );
    }
}
