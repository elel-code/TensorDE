#[path = "tasks/geometry.rs"]
pub(crate) mod geometry;
#[path = "tasks/paint.rs"]
pub(crate) mod paint;

pub(crate) type ShellTaskId = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellTaskStatusKind {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellTaskStatus {
    pub(crate) task_id: Option<ShellTaskId>,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) kind: ShellTaskStatusKind,
    pub(crate) privileged: bool,
    pub(crate) cancellable: bool,
}

impl ShellTaskStatus {
    pub(crate) fn running(
        task_id: ShellTaskId,
        label: impl Into<String>,
        detail: impl Into<String>,
        privileged: bool,
    ) -> Self {
        Self {
            task_id: Some(task_id),
            label: label.into(),
            detail: detail.into(),
            kind: ShellTaskStatusKind::Running,
            privileged,
            cancellable: true,
        }
    }

    pub(crate) fn running_uncancellable(
        task_id: ShellTaskId,
        label: impl Into<String>,
        detail: impl Into<String>,
        privileged: bool,
    ) -> Self {
        Self {
            task_id: Some(task_id),
            label: label.into(),
            detail: detail.into(),
            kind: ShellTaskStatusKind::Running,
            privileged,
            cancellable: false,
        }
    }

    pub(crate) fn completed(
        label: impl Into<String>,
        detail: impl Into<String>,
        privileged: bool,
    ) -> Self {
        Self {
            task_id: None,
            label: label.into(),
            detail: detail.into(),
            kind: ShellTaskStatusKind::Completed,
            privileged,
            cancellable: false,
        }
    }

    pub(crate) fn failed(
        label: impl Into<String>,
        detail: impl Into<String>,
        privileged: bool,
    ) -> Self {
        Self {
            task_id: None,
            label: label.into(),
            detail: detail.into(),
            kind: ShellTaskStatusKind::Failed,
            privileged,
            cancellable: false,
        }
    }

    pub(crate) fn cancelled(
        label: impl Into<String>,
        detail: impl Into<String>,
        privileged: bool,
    ) -> Self {
        Self {
            task_id: None,
            label: label.into(),
            detail: detail.into(),
            kind: ShellTaskStatusKind::Cancelled,
            privileged,
            cancellable: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellTaskDetailDialog;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskDetailDialogClick {
    Outside,
    Inside,
    Cancel,
    Clear,
    Dismiss(usize),
}
