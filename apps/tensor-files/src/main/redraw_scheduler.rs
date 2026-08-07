use crate::ShellRenderOutcome;

/// Tracks a scene change until one render transaction has successfully
/// presented it. Animation frames are scheduled independently by their
/// deadlines, so a scene change never needs an arbitrary frame-count budget.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellScenePresentState {
    pending: bool,
    reason: Option<&'static str>,
}

impl ShellScenePresentState {
    pub(crate) fn request(&mut self, reason: &'static str) {
        self.pending = true;
        self.reason = Some(reason);
    }

    pub(crate) fn pending(self) -> bool {
        self.pending
    }

    pub(crate) fn reason(self) -> Option<&'static str> {
        self.reason
    }

    pub(crate) fn complete(&mut self, outcome: ShellRenderOutcome) {
        if outcome.presented() {
            self.pending = false;
            self.reason = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_scene_change_is_consumed_by_one_present() {
        let mut state = ShellScenePresentState::default();
        state.request("view-mode");
        assert!(state.pending());
        assert_eq!(state.reason(), Some("view-mode"));

        state.complete(ShellRenderOutcome::Presented);
        assert!(!state.pending());
        assert_eq!(state.reason(), None);
    }

    #[test]
    fn deferred_render_keeps_scene_change_for_retry() {
        let mut state = ShellScenePresentState::default();
        state.request("surface-retry");

        state.complete(ShellRenderOutcome::NotReady);
        assert!(state.pending());
        assert_eq!(state.reason(), Some("surface-retry"));
    }

    #[test]
    fn newer_scene_change_replaces_log_reason_while_pending() {
        let mut state = ShellScenePresentState::default();
        state.request("first");
        state.request("second");
        assert_eq!(state.reason(), Some("second"));
    }
}
