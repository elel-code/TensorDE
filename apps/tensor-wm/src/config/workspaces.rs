use std::collections::HashSet;

use thiserror::Error;

pub(crate) const MAX_REGULAR_WORKSPACES: u32 = 32;
pub(crate) const MAX_HIDDEN_WORKSPACES: usize = 16;
const MAX_WORKSPACE_NAME_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceConfig {
    pub(crate) regular_count: u32,
    pub(crate) hidden: Vec<HiddenWorkspaceConfig>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            regular_count: 1,
            hidden: vec![HiddenWorkspaceConfig {
                name: "minimized".to_owned(),
                show_in_overview: true,
                minimize_target: true,
            }],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HiddenWorkspaceConfig {
    pub(crate) name: String,
    pub(crate) show_in_overview: bool,
    pub(crate) minimize_target: bool,
}

pub(super) fn resolve(
    regular_count: Option<u32>,
    configured_hidden: Vec<(String, Option<bool>, Option<bool>)>,
) -> Result<WorkspaceConfig, WorkspaceConfigError> {
    let defaults = WorkspaceConfig::default();
    let regular_count = regular_count.unwrap_or(defaults.regular_count);
    if !(1..=MAX_REGULAR_WORKSPACES).contains(&regular_count) {
        return Err(WorkspaceConfigError::RegularCount { regular_count });
    }
    if configured_hidden.is_empty() {
        return Ok(WorkspaceConfig {
            regular_count,
            hidden: defaults.hidden,
        });
    }
    if configured_hidden.len() > MAX_HIDDEN_WORKSPACES {
        return Err(WorkspaceConfigError::TooManyHidden {
            count: configured_hidden.len(),
        });
    }

    let mut names = HashSet::with_capacity(configured_hidden.len());
    let mut minimize_target = None;
    let mut hidden = Vec::with_capacity(configured_hidden.len());
    for (index, (name, show_in_overview, configured_minimize_target)) in
        configured_hidden.into_iter().enumerate()
    {
        if name.is_empty() || name.len() > MAX_WORKSPACE_NAME_BYTES {
            return Err(WorkspaceConfigError::InvalidName { name });
        }
        if !names.insert(name.clone()) {
            return Err(WorkspaceConfigError::DuplicateName { name });
        }
        let is_minimize_target = configured_minimize_target.unwrap_or(false);
        if is_minimize_target && minimize_target.replace(index).is_some() {
            return Err(WorkspaceConfigError::MultipleMinimizeTargets);
        }
        hidden.push(HiddenWorkspaceConfig {
            name,
            show_in_overview: show_in_overview.unwrap_or(true),
            minimize_target: is_minimize_target,
        });
    }
    if minimize_target.is_none() {
        return Err(WorkspaceConfigError::MissingMinimizeTarget);
    }
    Ok(WorkspaceConfig {
        regular_count,
        hidden,
    })
}

#[derive(Debug, Error)]
pub enum WorkspaceConfigError {
    #[error("default-count must be between 1 and {MAX_REGULAR_WORKSPACES}, got {regular_count}")]
    RegularCount { regular_count: u32 },
    #[error("at most {MAX_HIDDEN_WORKSPACES} hidden workspaces are supported, got {count}")]
    TooManyHidden { count: usize },
    #[error("hidden workspace name must contain 1..={MAX_WORKSPACE_NAME_BYTES} bytes: {name:?}")]
    InvalidName { name: String },
    #[error("hidden workspace {name:?} is configured more than once")]
    DuplicateName { name: String },
    #[error("one hidden workspace must set minimize-target=#true")]
    MissingMinimizeTarget,
    #[error("only one hidden workspace may set minimize-target=#true")]
    MultipleMinimizeTargets,
}
