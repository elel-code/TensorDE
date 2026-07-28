impl WgpuState {

    fn can_skip_clean_redraw(
        &self,
        reason: &str,
        force_log: bool,
        dirty_key: &ShellRenderDirtyKey,
        async_results_changed: bool,
    ) -> bool {
        self.frame_count > 0
            && clean_render_skip_reason_allowed(reason, force_log)
            && !self.render_work_pending
            && !async_results_changed
            && self
                .last_render_dirty_key
                .as_ref()
                .is_some_and(|last| last == dirty_key)
    }
}
