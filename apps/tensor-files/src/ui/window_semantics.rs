use crate::windowing::{ActiveEventLoop, WindowAttributes};

const TENSOR_FILES_WAYLAND_APP_ID: &str = "org.tensorde.TensorFiles";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellDialogWindowRole {
    Create,
    OpenWith,
    Properties,
    Rename,
    Settings,
    TaskDetail,
    TrashConflict,
}

impl ShellDialogWindowRole {
    #[cfg(test)]
    fn wayland_instance(self) -> &'static str {
        match self {
            Self::Create => "tensor-files-create-dialog",
            Self::OpenWith => "tensor-files-open-with-dialog",
            Self::Properties => "tensor-files-properties-dialog",
            Self::Rename => "tensor-files-rename-dialog",
            Self::Settings => "tensor-files-settings-dialog",
            Self::TaskDetail => "tensor-files-task-detail-dialog",
            Self::TrashConflict => "tensor-files-trash-conflict-dialog",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellWindowRole {
    Main,
    Dialog(ShellDialogWindowRole),
}

impl ShellWindowRole {
    #[cfg(test)]
    fn wayland_instance(self) -> &'static str {
        match self {
            Self::Main => "tensor-files-main",
            Self::Dialog(role) => role.wayland_instance(),
        }
    }
}

pub(crate) fn apply_window_semantics(
    _event_loop: &ActiveEventLoop,
    attrs: WindowAttributes,
    role: ShellWindowRole,
) -> WindowAttributes {
    let attrs = attrs.with_app_id(TENSOR_FILES_WAYLAND_APP_ID);
    match role {
        ShellWindowRole::Main => attrs,
        ShellWindowRole::Dialog(_) => attrs.with_dialog(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_roles_have_distinct_wayland_instances() {
        assert_eq!(
            ShellDialogWindowRole::Create.wayland_instance(),
            "tensor-files-create-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::OpenWith.wayland_instance(),
            "tensor-files-open-with-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Rename.wayland_instance(),
            "tensor-files-rename-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Properties.wayland_instance(),
            "tensor-files-properties-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Settings.wayland_instance(),
            "tensor-files-settings-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TaskDetail.wayland_instance(),
            "tensor-files-task-detail-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TrashConflict.wayland_instance(),
            "tensor-files-trash-conflict-dialog"
        );
    }

    #[test]
    fn main_window_uses_stable_wayland_instance() {
        assert_eq!(
            ShellWindowRole::Main.wayland_instance(),
            "tensor-files-main"
        );
    }
}
