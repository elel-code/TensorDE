use std::path::Path;

use super::super::file_ops;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SortRole {
    Name,
    Modified,
    Size,
    TrashOriginalPath,
    TrashDeletionTime,
}

impl SortRole {
    pub fn default_order(self) -> SortOrder {
        match self {
            Self::Name | Self::TrashOriginalPath => SortOrder::Ascending,
            Self::Modified | Self::Size | Self::TrashDeletionTime => SortOrder::Descending,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortDescriptor {
    pub role: SortRole,
    pub order: SortOrder,
    pub folders_first: bool,
    pub hidden_last: bool,
}

impl SortDescriptor {
    pub fn for_directory(directory: &Path) -> Self {
        if file_ops::is_trash_files_dir(directory) {
            Self {
                role: SortRole::TrashDeletionTime,
                order: SortOrder::Descending,
                folders_first: true,
                hidden_last: false,
            }
        } else {
            Self::default()
        }
    }
}

impl Default for SortDescriptor {
    fn default() -> Self {
        Self {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            folders_first: true,
            hidden_last: false,
        }
    }
}
