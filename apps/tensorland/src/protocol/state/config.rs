//! Live application of the restart-free configuration subset.

use crate::{config::Config, layout::LayoutEngine};

use super::RuntimeState;

impl RuntimeState {
    /// Apply policy that does not replace startup-owned devices or services.
    ///
    /// The configuration transaction rejects changes to the GPU, output
    /// topology rules, IPC socket, XWayland, systemd, environment, and startup
    /// commands before this method runs.
    pub(crate) fn apply_live_config(&mut self, config: &Config) {
        let layout = LayoutEngine::with_options(config.initial_layout, config.layout_options);
        let layout_changed = self.layout != layout;
        if layout_changed {
            self.layout = layout;
            self.world.reset_layout_states();
            self.reflow_default_workspace();
        }
        self.overview_options = config.overview_options;

        let appearance_changed = self.world.set_appearance(config.appearance);
        self.apply_runtime_policy(config.cursor.clone(), config.debug);

        if layout_changed || appearance_changed {
            self.queue_workspace_redraw_intent();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::Config,
        layout::{LayoutEngine, LayoutKind, LayoutOptions},
        protocol::test_runtime_state,
        scene::{FocusRingStyle, LinearRgba16, SceneAppearance},
    };

    #[test]
    fn live_config_replaces_layout_and_appearance_together() {
        let mut state = test_runtime_state(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        );
        let config = Config {
            initial_layout: LayoutKind::Spatial2D,
            layout_options: LayoutOptions {
                gap: 23,
                ..LayoutOptions::default()
            },
            appearance: SceneAppearance {
                focus_ring: FocusRingStyle {
                    enabled: true,
                    width: 7,
                    color: LinearRgba16::new(1, 2, 3, 4),
                },
                ..SceneAppearance::default()
            },
            ..Config::default()
        };

        state.apply_live_config(&config);

        assert_eq!(state.layout.kind(), LayoutKind::Spatial2D);
        assert_eq!(state.layout.options().gap, 23);
        assert_eq!(state.overview_options, config.overview_options);
        assert_eq!(state.world.appearance(), config.appearance);
    }
}
