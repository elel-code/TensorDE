use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::ops::Deref;

use crate::ui::pane::ShellPaneView;
use crate::ui::tasks::{ShellTaskId, ShellTaskStatus, ShellTaskStatusKind};

pub(crate) mod paint;

pub(crate) const MAX_TASK_STATUSES: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellPaneStatus {
    pub(crate) primary: String,
    pub(crate) qualifier: String,
}

impl ShellPaneStatus {
    pub(crate) fn for_view(
        pane: ShellPaneView<'_>,
        visible_items: usize,
        show_hidden: bool,
        filter_active: bool,
    ) -> Self {
        let total = pane.entries.len();
        let selected = pane.selection.len();
        let file_count = total.saturating_sub(pane.dir_count);
        let primary = if selected > 0 {
            format!("{} selected", count_label(selected, "item", "items"))
        } else {
            format!(
                "{}, {}, {}",
                count_label(total, "item", "items"),
                count_label(pane.dir_count, "folder", "folders"),
                count_label(file_count, "file", "files")
            )
        };
        let qualifier = pane_qualifier_text(pane, visible_items, total, show_hidden, filter_active);

        Self { primary, qualifier }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn plain_text(&self) -> String {
        if self.qualifier.is_empty() {
            self.primary.clone()
        } else {
            format!("{} | {}", self.primary, self.qualifier)
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn qualifier_text(&self) -> &str {
        &self.qualifier
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellTaskStatusDismissal {
    pub(crate) changed: bool,
    pub(crate) cancel_task_id: Option<ShellTaskId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellTaskStatusStore {
    statuses: VecDeque<ShellTaskStatus>,
    changes: u64,
}

impl ShellTaskStatusStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn change_generation(&self) -> u64 {
        self.changes
    }

    pub(crate) fn mark_changed(&mut self) {
        self.changes += 1;
    }

    pub(crate) fn record(&mut self, status: ShellTaskStatus) {
        self.statuses.push_front(status);
        while self.statuses.len() > MAX_TASK_STATUSES {
            self.statuses.pop_back();
        }
        self.mark_changed();
    }

    pub(crate) fn finish(&mut self, task_id: ShellTaskId, mut status: ShellTaskStatus) {
        status.task_id = Some(task_id);
        if let Some(existing) = self
            .statuses
            .iter_mut()
            .find(|candidate| candidate.task_id == Some(task_id))
        {
            *existing = status;
            self.mark_changed();
        } else {
            self.record(status);
        }
    }

    pub(crate) fn update_running_detail(&mut self, task_id: ShellTaskId, detail: String) -> bool {
        let Some(status) = self
            .statuses
            .iter_mut()
            .find(|status| status.task_id == Some(task_id))
        else {
            return false;
        };
        if status.kind != ShellTaskStatusKind::Running || status.detail == detail {
            return false;
        }
        status.detail = detail;
        self.mark_changed();
        true
    }

    pub(crate) fn clear_finished(&mut self) -> bool {
        let old_statuses = self.statuses.clone();
        self.statuses
            .retain(|status| status.kind == ShellTaskStatusKind::Running);
        if self.statuses == old_statuses {
            return false;
        }
        self.mark_changed();
        true
    }

    pub(crate) fn dismiss(&mut self, index: usize) -> ShellTaskStatusDismissal {
        if index >= self.statuses.len() {
            return ShellTaskStatusDismissal::default();
        }
        let task_id = self.statuses[index].task_id;
        if self.statuses[index].kind == ShellTaskStatusKind::Running
            && self.statuses[index].cancellable
        {
            self.statuses[index] = ShellTaskStatus::cancelled(
                "Task cancelling",
                self.statuses[index].detail.clone(),
                self.statuses[index].privileged,
            );
            self.statuses[index].task_id = task_id;
            self.mark_changed();
            return ShellTaskStatusDismissal {
                changed: true,
                cancel_task_id: task_id,
            };
        }

        self.statuses.remove(index);
        self.mark_changed();
        ShellTaskStatusDismissal {
            changed: true,
            cancel_task_id: None,
        }
    }
}

impl Deref for ShellTaskStatusStore {
    type Target = VecDeque<ShellTaskStatus>;

    fn deref(&self) -> &Self::Target {
        &self.statuses
    }
}

impl ShellTaskStatusKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl ShellTaskStatus {
    pub(crate) fn detail_label(&self) -> Cow<'_, str> {
        if self.privileged {
            Cow::Owned(format!("{} | administrator", self.detail))
        } else {
            Cow::Borrowed(&self.detail)
        }
    }
}

fn count_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn pane_qualifier_text(
    pane: ShellPaneView<'_>,
    visible_items: usize,
    total: usize,
    show_hidden: bool,
    filter_active: bool,
) -> String {
    let mut qualifier = String::new();
    if visible_items != total {
        append_qualifier_separator(&mut qualifier);
        let _ = write!(&mut qualifier, "{visible_items} visible");
    }
    if show_hidden {
        append_qualifier(&mut qualifier, "hidden shown");
    }
    if filter_active {
        append_qualifier_separator(&mut qualifier);
        let _ = write!(&mut qualifier, "{} matches", pane.filtered_entry_count());
    }
    qualifier
}

fn append_qualifier(qualifier: &mut String, label: &str) {
    append_qualifier_separator(qualifier);
    qualifier.push_str(label);
}

fn append_qualifier_separator(qualifier: &mut String) {
    if !qualifier.is_empty() {
        qualifier.push_str(" | ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_store_caps_and_updates_running_tasks() {
        let mut store = ShellTaskStatusStore::new();
        for index in 0..6 {
            store.record(ShellTaskStatus::completed(
                format!("Task {index}"),
                "done",
                false,
            ));
        }

        assert_eq!(store.len(), MAX_TASK_STATUSES);
        assert_eq!(store[0].label, "Task 5");
        assert_eq!(store[MAX_TASK_STATUSES - 1].label, "Task 2");
        assert_eq!(store.change_generation(), 6);

        store.record(ShellTaskStatus::running(77, "Copying", "1 item", false));
        assert!(store.update_running_detail(77, "2 items".to_string()));
        assert_eq!(store[0].detail, "2 items");

        store.finish(77, ShellTaskStatus::completed("Copied", "2 items", false));
        assert_eq!(store[0].kind, ShellTaskStatusKind::Completed);
        assert_eq!(store[0].task_id, Some(77));
    }

    #[test]
    fn task_status_store_cancels_running_dismissal() {
        let mut store = ShellTaskStatusStore::new();
        store.record(ShellTaskStatus::running(3, "Moving", "alpha", false));

        let dismissal = store.dismiss(0);

        assert!(dismissal.changed);
        assert_eq!(dismissal.cancel_task_id, Some(3));
        assert_eq!(store[0].kind, ShellTaskStatusKind::Cancelled);
        assert_eq!(store[0].task_id, Some(3));
    }
}
