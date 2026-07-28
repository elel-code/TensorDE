//! Renderer-independent completion boundary for directory navigation.

use crate::ShellScene;
use crate::ui::tasks::ShellTaskStatus;
use crate::ui::transfer::{ShellAsyncNavigationCompletion, ShellNavigationHistoryUpdate};
use crate::windowing::PhysicalSize;

/// Commits one background listing only while it still belongs to the
/// displayed transaction, so stale work cannot repopulate a retargeted pane.
pub(crate) fn apply_navigation_completion(
    scene: &mut ShellScene,
    navigation_generations: &[u64; 2],
    completion: ShellAsyncNavigationCompletion,
    size: PhysicalSize<u32>,
) -> bool {
    let pane = completion.pane;
    if navigation_generations[pane.index()] != completion.generation {
        return false;
    }
    if !scene
        .pane_state(pane)
        .is_some_and(|state| state.path == completion.source_path)
    {
        return false;
    }
    if !scene.pending_pane_navigation_matches(pane, &completion.target_path) {
        return false;
    }

    let entries = match completion.result {
        Ok(entries) => entries,
        Err(error) => {
            if crate::fika_log_enabled() {
                eprintln!(
                    "[fika] async-navigation-error reason={} path={} error={error}",
                    completion.reason,
                    completion.target_path.display()
                );
            }
            let _ = scene.cancel_pane_navigation(pane);
            scene.record_task_status(ShellTaskStatus::failed("Open folder failed", error, false));
            return true;
        }
    };

    let history = scene.pane_history_mut(pane);
    match completion.history {
        ShellNavigationHistoryUpdate::Push => {
            history.push_back(completion.source_path);
            history.clear_forward();
        }
        ShellNavigationHistoryUpdate::Back => {
            if history.back.last() == Some(&completion.target_path) {
                history.back.pop();
            }
            history.push_forward(completion.source_path);
        }
        ShellNavigationHistoryUpdate::Forward => {
            if history.forward.last() == Some(&completion.target_path) {
                history.forward.pop();
            }
            history.push_back(completion.source_path);
        }
    }
    scene.complete_pane_navigation(pane, completion.target_path, entries, size)
}
