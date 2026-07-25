use std::path::{Path, PathBuf};

use crate::shell::pane::ShellPaneId;
use crate::shell::text_input::ShellTextPreedit;

#[path = "create_rename/disk.rs"]
pub(crate) mod disk;
#[path = "create_rename/geometry.rs"]
pub(crate) mod geometry;
#[path = "create_rename/paint.rs"]
pub(crate) mod paint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreateEntryKind {
    Folder,
    File,
}

impl CreateEntryKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::File => "File",
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
        }
    }

    pub(crate) fn admin_as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder-as-administrator",
            Self::File => "file-as-administrator",
        }
    }

    pub(crate) fn default_name(self) -> &'static str {
        match self {
            Self::Folder => "New Folder",
            Self::File => "New File",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellCreateDialog {
    pub(crate) pane: ShellPaneId,
    pub(crate) parent: PathBuf,
    pub(crate) kind: CreateEntryKind,
    pub(crate) privileged: bool,
    pub(crate) name: String,
    pub(crate) error: Option<String>,
    pub(crate) busy: bool,
    pub(crate) replace_on_insert: bool,
    pub(crate) preedit: Option<ShellTextPreedit>,
}

impl ShellCreateDialog {
    pub(crate) fn new(
        pane: ShellPaneId,
        parent: PathBuf,
        kind: CreateEntryKind,
        privileged: bool,
    ) -> Self {
        let name = unique_child_name(&parent, kind.default_name());
        Self {
            pane,
            parent,
            kind,
            privileged,
            name,
            error: None,
            busy: false,
            replace_on_insert: true,
            preedit: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreateEntryRequest {
    pub(crate) pane: ShellPaneId,
    pub(crate) parent: PathBuf,
    pub(crate) path: PathBuf,
    pub(crate) kind: CreateEntryKind,
    pub(crate) name: String,
    pub(crate) privileged: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreateDialogClick {
    Outside,
    Inside,
    Cancel,
    Commit,
    Kind(CreateEntryKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellRenameDialog {
    pub(crate) pane: ShellPaneId,
    pub(crate) source: PathBuf,
    pub(crate) parent: PathBuf,
    pub(crate) original_name: String,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) privileged: bool,
    pub(crate) error: Option<String>,
    pub(crate) busy: bool,
    pub(crate) replace_on_insert: bool,
    pub(crate) preedit: Option<ShellTextPreedit>,
}

impl ShellRenameDialog {
    pub(crate) fn new(
        pane: ShellPaneId,
        source: PathBuf,
        is_dir: bool,
        privileged: bool,
    ) -> Option<Self> {
        let parent = source.parent()?.to_path_buf();
        let original_name = source.file_name()?.to_string_lossy().to_string();
        Some(Self {
            pane,
            source,
            parent,
            name: original_name.clone(),
            original_name,
            is_dir,
            privileged,
            error: None,
            busy: false,
            replace_on_insert: true,
            preedit: None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenameEntryRequest {
    pub(crate) pane: ShellPaneId,
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) original_name: String,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) privileged: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenameDialogClick {
    Outside,
    Inside,
    Cancel,
    Commit,
}

pub(crate) fn validate_create_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    if name == "." || name == ".." {
        return Err("name must not be . or ..".to_string());
    }
    if name.contains('/') {
        return Err("name must not contain /".to_string());
    }
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    Ok(())
}

pub(crate) fn unique_child_name(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
    }
    for suffix in 2..1000 {
        let candidate = format!("{base} {suffix}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    base.to_string()
}
