use fika_core::file_ops;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellTrashConflictDialog {
    pub(crate) conflicts: Vec<file_ops::TrashRestoreConflict>,
}

impl ShellTrashConflictDialog {
    pub(crate) fn new(conflicts: Vec<file_ops::TrashRestoreConflict>) -> Option<Self> {
        (!conflicts.is_empty()).then_some(Self { conflicts })
    }

    pub(crate) fn first_conflict(&self) -> Option<&file_ops::TrashRestoreConflict> {
        self.conflicts.first()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrashConflictDialogClick {
    Outside,
    Inside,
    Cancel,
    Replace,
}
