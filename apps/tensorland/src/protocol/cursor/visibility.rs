use std::time::{Duration, Instant};

use super::CursorState;

impl CursorState {
    pub(crate) fn configure_from(
        &mut self,
        config: crate::config::CursorConfig,
    ) -> (Vec<crate::ecs::SurfaceBufferId>, bool) {
        self.configure(
            config.theme,
            config.size,
            config.hide_when_typing,
            config.hide_after_inactive_ms,
        )
    }

    pub(super) fn configure_visibility(
        &mut self,
        hide_when_typing: bool,
        hide_after_inactive_ms: Option<u32>,
        now: Instant,
    ) {
        self.hide_when_typing = hide_when_typing;
        if !hide_when_typing {
            self.hidden_for_typing = false;
        }
        self.hide_after_inactive =
            hide_after_inactive_ms.map(|millis| Duration::from_millis(u64::from(millis)));
        self.inactivity_deadline = self
            .hide_after_inactive
            .and_then(|timeout| now.checked_add(timeout));
        self.arm_pending_inactivity(now);
    }

    pub(crate) const fn pointer_is_visible(&self) -> bool {
        !self.hidden_for_typing && !self.hidden_for_inactivity
    }

    pub(crate) const fn tablets_are_visible(&self) -> bool {
        !self.hidden_for_inactivity
    }

    /// Hide the pointer cursor after a keyboard press when configured. Active
    /// tablet cursors remain visible, matching Niri's anti-flicker policy.
    pub(crate) const fn will_hide_for_keyboard_activity(&self) -> bool {
        self.hide_when_typing
            && !self.hidden_for_typing
            && !self.hidden_for_inactivity
            && self.tablets.is_empty()
    }

    pub(crate) fn note_keyboard_activity(&mut self) -> bool {
        if !self.will_hide_for_keyboard_activity() {
            return false;
        }
        self.hidden_for_typing = true;
        true
    }

    /// Reveal cursor overlays and move the inactivity deadline. The shared
    /// timer keeps an earlier arm instead of issuing a timerfd syscall for
    /// every high-frequency motion sample; an early wake validates this
    /// deadline and rearms once when necessary.
    pub(crate) fn note_pointer_activity(&mut self, now: Instant) -> bool {
        let changed = self.hidden_for_typing || self.hidden_for_inactivity;
        self.hidden_for_typing = false;
        self.hidden_for_inactivity = false;
        self.inactivity_deadline = self
            .hide_after_inactive
            .and_then(|timeout| now.checked_add(timeout));
        self.arm_pending_inactivity(now);
        changed
    }

    pub(crate) fn inactivity_will_hide(&self, now: Instant) -> bool {
        !self.hidden_for_inactivity
            && self
                .inactivity_deadline
                .is_some_and(|deadline| deadline <= now)
    }

    pub(crate) fn expire_inactivity(&mut self, now: Instant) -> bool {
        if !self.inactivity_will_hide(now) {
            return false;
        }
        self.inactivity_deadline = None;
        self.hidden_for_inactivity = true;
        true
    }

    pub(crate) fn arm_pending_inactivity(&mut self, now: Instant) {
        let Some(deadline) = self.inactivity_deadline else {
            return;
        };
        self.arm_cursor_timer(now, deadline.saturating_duration_since(now));
    }
}
