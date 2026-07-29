use std::{fmt, path::PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    InvalidStage(String),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    ToolLaunch {
        tool: PathBuf,
        source: std::io::Error,
    },
    ToolFailure {
        tool: PathBuf,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    CompilerVersion {
        expected: &'static str,
        found: String,
    },
    Reflection(String),
    SourceLowering(String),
    SpirvContract(String),
    ArtifactMismatch {
        path: PathBuf,
        expected_bytes: usize,
        generated_bytes: usize,
    },
}

impl Error {
    pub(crate) fn io(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStage(stage) => write!(formatter, "unsupported shader stage `{stage}`"),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::ToolLaunch { tool, source } => {
                write!(formatter, "failed to launch {}: {source}", tool.display())
            }
            Self::ToolFailure {
                tool,
                status,
                stdout,
                stderr,
            } => write!(
                formatter,
                "{} exited with {status:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                tool.display()
            ),
            Self::CompilerVersion { expected, found } => write!(
                formatter,
                "Slang compiler version mismatch: expected {expected}, found {found}"
            ),
            Self::Reflection(message) => write!(formatter, "shader reflection mismatch: {message}"),
            Self::SourceLowering(message) => {
                write!(formatter, "shader source lowering failed: {message}")
            }
            Self::SpirvContract(message) => {
                write!(formatter, "SPIR-V contract mismatch: {message}")
            }
            Self::ArtifactMismatch {
                path,
                expected_bytes,
                generated_bytes,
            } => write!(
                formatter,
                "checked-in SPIR-V {} differs from generated output ({expected_bytes} versus {generated_bytes} bytes)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::ToolLaunch { source, .. } => Some(source),
            _ => None,
        }
    }
}
