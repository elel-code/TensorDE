use std::collections::VecDeque;
use std::env;
use std::time::Duration;

use crate::ui::shortcuts::ZoomAction;

pub(crate) struct AutosmokeZoomConfig {
    pub(crate) actions: VecDeque<ZoomAction>,
    pub(crate) interval: Duration,
    pub(crate) allow_pending_redraw: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AutosmokeScrollAction {
    pub(crate) delta: f32,
    pub(crate) label: &'static str,
}

pub(crate) struct AutosmokeScrollConfig {
    pub(crate) actions: VecDeque<AutosmokeScrollAction>,
    pub(crate) interval: Duration,
    pub(crate) allow_pending_redraw: bool,
}

pub(crate) fn autosmoke_zoom_config() -> AutosmokeZoomConfig {
    autosmoke_zoom_config_from_env(&env_value)
}

pub(crate) fn autosmoke_scroll_config(default_step: f32) -> AutosmokeScrollConfig {
    autosmoke_scroll_config_from_env(&env_value, default_step)
}

fn env_value(key: &str) -> Option<String> {
    env::var_os(key).map(|value| value.to_string_lossy().into_owned())
}

fn autosmoke_zoom_config_from_env(
    get_env: &impl Fn(&str) -> Option<String>,
) -> AutosmokeZoomConfig {
    let enabled = env_flag_enabled(get_env, "FIKA_WGPU_AUTOSMOKE_ZOOM");
    if !enabled {
        return AutosmokeZoomConfig {
            actions: VecDeque::new(),
            interval: Duration::from_millis(250),
            allow_pending_redraw: false,
        };
    }

    let rapid = env_flag_enabled(get_env, "FIKA_WGPU_AUTOSMOKE_ZOOM_RAPID");
    let interval = env_duration_millis(get_env, "FIKA_WGPU_AUTOSMOKE_ZOOM_INTERVAL_MS")
        .unwrap_or_else(|| Duration::from_millis(if rapid { 16 } else { 250 }));
    let actions = if rapid {
        let mut actions = VecDeque::new();
        for _ in 0..6 {
            actions.push_back(ZoomAction::In);
            actions.push_back(ZoomAction::In);
            actions.push_back(ZoomAction::Out);
            actions.push_back(ZoomAction::Out);
        }
        actions.push_back(ZoomAction::Reset);
        actions
    } else {
        VecDeque::from([
            ZoomAction::In,
            ZoomAction::In,
            ZoomAction::Out,
            ZoomAction::Reset,
        ])
    };
    AutosmokeZoomConfig {
        actions,
        interval,
        allow_pending_redraw: rapid,
    }
}

fn autosmoke_scroll_config_from_env(
    get_env: &impl Fn(&str) -> Option<String>,
    default_step: f32,
) -> AutosmokeScrollConfig {
    let enabled = env_flag_enabled(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL");
    if !enabled {
        return AutosmokeScrollConfig {
            actions: VecDeque::new(),
            interval: Duration::from_millis(250),
            allow_pending_redraw: false,
        };
    }

    let rapid = env_flag_enabled(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL_RAPID");
    let interval = env_duration_millis(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL_INTERVAL_MS")
        .unwrap_or_else(|| Duration::from_millis(if rapid { 16 } else { 120 }));
    let step = env_f32(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL_STEP").unwrap_or(default_step);
    let mut actions = VecDeque::new();
    let forward_count = env_usize(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL_FORWARD_COUNT")
        .unwrap_or(if rapid { 28 } else { 10 });
    let back_count = env_usize(get_env, "FIKA_WGPU_AUTOSMOKE_SCROLL_BACK_COUNT")
        .unwrap_or(if rapid { 14 } else { 5 });
    for _ in 0..forward_count {
        actions.push_back(AutosmokeScrollAction {
            delta: step,
            label: "forward",
        });
    }
    for _ in 0..back_count {
        actions.push_back(AutosmokeScrollAction {
            delta: -step,
            label: "back",
        });
    }
    AutosmokeScrollConfig {
        actions,
        interval,
        allow_pending_redraw: rapid,
    }
}

fn env_flag_enabled(get_env: &impl Fn(&str) -> Option<String>, key: &str) -> bool {
    get_env(key).is_some_and(|value| {
        let value = value.trim().to_ascii_lowercase();
        !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
    })
}

fn env_duration_millis(get_env: &impl Fn(&str) -> Option<String>, key: &str) -> Option<Duration> {
    get_env(key)
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
}

fn env_f32(get_env: &impl Fn(&str) -> Option<String>, key: &str) -> Option<f32> {
    get_env(key).and_then(|value| value.trim().parse::<f32>().ok())
}

fn env_usize(get_env: &impl Fn(&str) -> Option<String>, key: &str) -> Option<usize> {
    get_env(key).and_then(|value| value.trim().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map_env(values: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<HashMap<_, _>>();
        move |key| values.get(key).cloned()
    }

    #[test]
    fn autosmoke_zoom_config_uses_rapid_sequence_and_interval() {
        let env = map_env(&[
            ("FIKA_WGPU_AUTOSMOKE_ZOOM", "1"),
            ("FIKA_WGPU_AUTOSMOKE_ZOOM_RAPID", "true"),
        ]);
        let config = autosmoke_zoom_config_from_env(&env);

        assert!(config.allow_pending_redraw);
        assert_eq!(config.interval, Duration::from_millis(16));
        assert_eq!(config.actions.len(), 25);
        assert_eq!(config.actions.front(), Some(&ZoomAction::In));
        assert_eq!(config.actions.back(), Some(&ZoomAction::Reset));
    }

    #[test]
    fn autosmoke_scroll_config_uses_counts_and_step() {
        let env = map_env(&[
            ("FIKA_WGPU_AUTOSMOKE_SCROLL", "1"),
            ("FIKA_WGPU_AUTOSMOKE_SCROLL_STEP", "160"),
            ("FIKA_WGPU_AUTOSMOKE_SCROLL_FORWARD_COUNT", "2"),
            ("FIKA_WGPU_AUTOSMOKE_SCROLL_BACK_COUNT", "1"),
            ("FIKA_WGPU_AUTOSMOKE_SCROLL_INTERVAL_MS", "42"),
        ]);
        let config = autosmoke_scroll_config_from_env(&env, 32.0);

        assert!(!config.allow_pending_redraw);
        assert_eq!(config.interval, Duration::from_millis(42));
        let actions = config.actions.into_iter().collect::<Vec<_>>();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].label, "forward");
        assert_eq!(actions[0].delta, 160.0);
        assert_eq!(actions[1].label, "forward");
        assert_eq!(actions[1].delta, 160.0);
        assert_eq!(actions[2].label, "back");
        assert_eq!(actions[2].delta, -160.0);
    }

    #[test]
    fn env_flag_enabled_rejects_false_like_values() {
        for value in ["", "0", "false", "no", "off"] {
            let env = map_env(&[("FLAG", value)]);
            assert!(!env_flag_enabled(&env, "FLAG"));
        }
        let env = map_env(&[("FLAG", "yes")]);
        assert!(env_flag_enabled(&env, "FLAG"));
    }
}
