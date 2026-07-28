use std::path::PathBuf;

use fika_core::{FileTransferMode, ViewPoint};

use crate::ui::pane::ShellPaneId;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellDropTarget {
    PaneItem {
        pane: ShellPaneId,
        index: usize,
        path: PathBuf,
        is_dir: bool,
    },
    PaneBlank {
        pane: ShellPaneId,
        path: PathBuf,
    },
    Place {
        index: usize,
        path: PathBuf,
    },
    PlacesGap {
        index: usize,
    },
    PlacesBlank,
}

impl ShellDropTarget {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::PaneItem { .. } => "pane-item",
            Self::PaneBlank { .. } => "pane-blank",
            Self::Place { .. } => "place",
            Self::PlacesGap { .. } => "places-gap",
            Self::PlacesBlank => "places-blank",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellDropMenuCommand {
    Mode {
        mode: FileTransferMode,
        privileged: bool,
    },
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellDropMenuIcon {
    Copy,
    Move,
    Link,
    Cancel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellDropMenuItem {
    pub(crate) command: ShellDropMenuCommand,
    pub(crate) label: &'static str,
    pub(crate) icon: ShellDropMenuIcon,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShellDropMenu {
    pub(crate) sources: Vec<PathBuf>,
    pub(crate) target_dir: PathBuf,
    pub(crate) target: ShellDropTarget,
    pub(crate) position: ViewPoint,
    pub(crate) hovered_row: Option<usize>,
}

impl ShellDropMenu {
    pub(crate) fn new(
        sources: Vec<PathBuf>,
        target_dir: PathBuf,
        target: ShellDropTarget,
        position: ViewPoint,
    ) -> Self {
        Self {
            sources,
            target_dir,
            target,
            position,
            hovered_row: None,
        }
    }
}

pub(crate) fn drop_menu_items() -> [ShellDropMenuItem; 7] {
    [
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Copy,
                privileged: false,
            },
            label: "Copy Here",
            icon: ShellDropMenuIcon::Copy,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Move,
                privileged: false,
            },
            label: "Move Here",
            icon: ShellDropMenuIcon::Move,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Link,
                privileged: false,
            },
            label: "Link Here",
            icon: ShellDropMenuIcon::Link,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Copy,
                privileged: true,
            },
            label: "Copy Here as Administrator",
            icon: ShellDropMenuIcon::Copy,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Move,
                privileged: true,
            },
            label: "Move Here as Administrator",
            icon: ShellDropMenuIcon::Move,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Mode {
                mode: FileTransferMode::Link,
                privileged: true,
            },
            label: "Link Here as Administrator",
            icon: ShellDropMenuIcon::Link,
        },
        ShellDropMenuItem {
            command: ShellDropMenuCommand::Cancel,
            label: "Cancel",
            icon: ShellDropMenuIcon::Cancel,
        },
    ]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellDropOperationRequest {
    pub(crate) sources: Vec<PathBuf>,
    pub(crate) target_dir: PathBuf,
    pub(crate) target: ShellDropTarget,
    pub(crate) mode: FileTransferMode,
    pub(crate) privileged: bool,
}
