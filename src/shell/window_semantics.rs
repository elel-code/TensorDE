use crate::windowing::{ActiveEventLoop, WindowAttributes};

const FIKA_WAYLAND_APP_ID: &str = "fika";

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
            Self::Create => "fika-create-dialog",
            Self::OpenWith => "fika-open-with-dialog",
            Self::Properties => "fika-properties-dialog",
            Self::Rename => "fika-rename-dialog",
            Self::Settings => "fika-settings-dialog",
            Self::TaskDetail => "fika-task-detail-dialog",
            Self::TrashConflict => "fika-trash-conflict-dialog",
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
            Self::Main => "fika-main",
            Self::Dialog(role) => role.wayland_instance(),
        }
    }
}

pub(crate) fn apply_window_semantics(
    _event_loop: &ActiveEventLoop,
    attrs: WindowAttributes,
    role: ShellWindowRole,
) -> WindowAttributes {
    let attrs = attrs.with_app_id(FIKA_WAYLAND_APP_ID);
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
            "fika-create-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::OpenWith.wayland_instance(),
            "fika-open-with-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Rename.wayland_instance(),
            "fika-rename-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Properties.wayland_instance(),
            "fika-properties-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::Settings.wayland_instance(),
            "fika-settings-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TaskDetail.wayland_instance(),
            "fika-task-detail-dialog"
        );
        assert_eq!(
            ShellDialogWindowRole::TrashConflict.wayland_instance(),
            "fika-trash-conflict-dialog"
        );
    }

    #[test]
    fn main_window_uses_stable_wayland_instance() {
        assert_eq!(ShellWindowRole::Main.wayland_instance(), "fika-main");
    }
}
