use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

pub use tensor_ipc::land::{ConfigDiagnosticCategory, ConfigDiagnosticMetadata};

pub const MAX_DIAGNOSTIC_PATH_BYTES: usize = 4_096;
pub const MAX_DIAGNOSTIC_SUMMARY_BYTES: usize = 512;
pub const MAX_VALIDATION_COMMAND_BYTES: usize = 16_512;

/// A KDL failure that retains its source and parser context for presentation.
///
/// Keeping this value structured lets startup render a detailed terminal
/// report and lets the reload worker lower a bounded IPC event without
/// reparsing the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    path: PathBuf,
    source: String,
    error: tensor_kdl::ErrorCtx,
}

impl ConfigDiagnostic {
    pub(super) fn new(path: &Path, source: &str, error: tensor_kdl::ErrorCtx) -> Self {
        Self {
            path: path.to_owned(),
            source: source.to_owned(),
            error,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source_text(&self) -> &str {
        &self.source
    }

    pub fn error_context(&self) -> &tensor_kdl::ErrorCtx {
        &self.error
    }

    pub fn line_column(&self) -> (usize, usize) {
        self.error.line_col(&self.source)
    }

    pub fn compact(&self) -> String {
        tensor_kdl::format_error_named(&self.error, &self.source, &self.path.display().to_string())
    }

    pub fn report(&self) -> miette::Report {
        tensor_kdl::report_error_named(&self.error, &self.source, &self.path.display().to_string())
    }

    pub fn metadata(&self) -> ConfigDiagnosticMetadata {
        let (line, column) = self.line_column();
        let path = bounded_path(&self.path);
        ConfigDiagnosticMetadata {
            category: category_for_error_code(self.error.code),
            error_code: self.error.code.as_str().to_owned(),
            line: Some(saturating_u32(line)),
            column: Some(saturating_u32(column)),
            summary: bounded_prefix(
                self.error
                    .message
                    .as_deref()
                    .unwrap_or_else(|| self.error.code.as_str()),
                MAX_DIAGNOSTIC_SUMMARY_BYTES,
            ),
            validation_command: validation_command(&path),
            path,
        }
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.compact())
    }
}

impl Error for ConfigDiagnostic {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

pub(super) fn metadata_for_config_error(
    path: &Path,
    error: &super::ConfigError,
) -> ConfigDiagnosticMetadata {
    if let super::ConfigError::Parse(diagnostic) = error {
        return diagnostic.metadata();
    }
    let path = bounded_path(path);
    let (category, error_code) = match error {
        super::ConfigError::Read { .. } => (ConfigDiagnosticCategory::Io, "read"),
        super::ConfigError::LegacyToml { .. } => (ConfigDiagnosticCategory::Policy, "legacy_toml"),
        super::ConfigError::ReloadRequiresRestart { .. } => {
            (ConfigDiagnosticCategory::Policy, "reload_requires_restart")
        }
        _ => (ConfigDiagnosticCategory::Policy, "invalid_policy"),
    };
    ConfigDiagnosticMetadata {
        category,
        path: path.clone(),
        error_code: error_code.to_owned(),
        line: None,
        column: None,
        summary: bounded_prefix(&error.to_string(), MAX_DIAGNOSTIC_SUMMARY_BYTES),
        validation_command: validation_command(&path),
    }
}

fn category_for_error_code(code: tensor_kdl::ErrorCode) -> ConfigDiagnosticCategory {
    use tensor_kdl::ErrorCode;
    match code {
        ErrorCode::TypeMismatch | ErrorCode::InvalidNumber | ErrorCode::InvalidKeyword => {
            ConfigDiagnosticCategory::Type
        }
        ErrorCode::UnknownProperty
        | ErrorCode::UnknownChild
        | ErrorCode::MissingProperty
        | ErrorCode::MissingArgument
        | ErrorCode::MissingChild
        | ErrorCode::DuplicateProperty
        | ErrorCode::ExceededLimit => ConfigDiagnosticCategory::Policy,
        _ => ConfigDiagnosticCategory::Syntax,
    }
}

fn bounded_path(path: &Path) -> String {
    bounded_suffix(&path.to_string_lossy(), MAX_DIAGNOSTIC_PATH_BYTES)
}

fn validation_command(path: &str) -> String {
    bounded_prefix(
        &format!("tensor --validate-config --config {}", shell_quote(path)),
        MAX_VALIDATION_COMMAND_BYTES,
    )
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.'))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn bounded_prefix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let marker = "...";
    let mut end = max_bytes.saturating_sub(marker.len()).min(value.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}{}", &value[..end], marker)
}

fn bounded_suffix(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let marker = "...";
    let mut start = value
        .len()
        .saturating_sub(max_bytes.saturating_sub(marker.len()));
    while !value.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    format!("{}{}", marker, &value[start..])
}
