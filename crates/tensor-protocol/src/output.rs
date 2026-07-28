use std::collections::HashMap;

use tensor_util::OutputScale;

/// Value-only output head advertised by a wire adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputHeadSnapshot {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: OutputScale,
    pub mode_width: i32,
    pub mode_height: i32,
    pub refresh_millihertz: i32,
    pub enabled: bool,
}

/// Partial output mutation. Unset fields preserve the current rule.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutputHeadUpdate {
    pub enabled: Option<bool>,
    pub position: Option<(i32, i32)>,
    pub scale: Option<OutputScale>,
}

/// Whether applying a partial configuration leaves at least one known head on.
pub fn configuration_keeps_head_enabled(
    heads: &HashMap<String, OutputHeadSnapshot>,
    updates: &HashMap<String, OutputHeadUpdate>,
) -> bool {
    heads.values().any(|head| {
        updates
            .get(&head.name)
            .and_then(|update| update.enabled)
            .unwrap_or(head.enabled)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(name: &str, enabled: bool) -> OutputHeadSnapshot {
        OutputHeadSnapshot {
            name: name.to_owned(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: OutputScale::from_f64(1.25).unwrap(),
            mode_width: 1920,
            mode_height: 1080,
            refresh_millihertz: 60_000,
            enabled,
        }
    }

    #[test]
    fn fixed_point_scale_makes_snapshot_equality_exact() {
        let a = head("DP-1", true);
        let mut b = a.clone();
        assert_eq!(a, b);
        b.scale = OutputScale::from_f64(1.5).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn disabling_one_head_preserves_an_unmentioned_active_head() {
        let heads = [
            ("DP-1".to_owned(), head("DP-1", true)),
            ("HDMI-A-1".to_owned(), head("HDMI-A-1", true)),
        ]
        .into_iter()
        .collect();
        let updates = [(
            "DP-1".to_owned(),
            OutputHeadUpdate {
                enabled: Some(false),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect();
        assert!(configuration_keeps_head_enabled(&heads, &updates));
    }

    #[test]
    fn disabling_every_current_head_is_rejected() {
        let heads = [
            ("DP-1".to_owned(), head("DP-1", true)),
            ("HDMI-A-1".to_owned(), head("HDMI-A-1", true)),
        ]
        .into_iter()
        .collect();
        let disable = |name: &str| {
            (
                name.to_owned(),
                OutputHeadUpdate {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
        };
        let updates = [disable("DP-1"), disable("HDMI-A-1")].into_iter().collect();
        assert!(!configuration_keeps_head_enabled(&heads, &updates));
    }
}
