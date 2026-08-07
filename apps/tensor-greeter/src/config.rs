#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use tensor_kdl::Decode;

pub const MAX_USERS: usize = 256;
pub const MAX_SESSIONS: usize = 64;
pub const MAX_AUTH_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreeterConfig {
    pub greetd_socket: PathBuf,
    pub sessions: Vec<SessionDefinition>,
    pub max_users: usize,
    pub max_auth_message_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionDefinition {
    pub id: String,
    pub label: String,
    pub command: Vec<String>,
    pub environment: Vec<String>,
}

impl GreeterConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_GREETER_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/greeter.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/greeter.kdl"))
    }

    pub fn load_default_path() -> Result<Self, GreeterConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, GreeterConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(GreeterConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, GreeterConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| GreeterConfigError::Parse {
                path: path.to_owned(),
                message: tensor_kdl::format_error_named(
                    &error,
                    document,
                    &path.display().to_string(),
                ),
            })?;
        parsed.resolve()
    }
}

impl Default for GreeterConfig {
    fn default() -> Self {
        Self {
            greetd_socket: env::var_os("GREETD_SOCK")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/run/greetd.sock")),
            sessions: vec![SessionDefinition {
                id: "tensorland".into(),
                label: "Tensorland".into(),
                command: vec!["tensor-session".into()],
                environment: vec![
                    "XDG_CURRENT_DESKTOP=Tensorland".into(),
                    "XDG_SESSION_DESKTOP=Tensorland".into(),
                    "XDG_SESSION_TYPE=wayland".into(),
                ],
            }],
            max_users: 128,
            max_auth_message_bytes: 4096,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child(name = "greetd-socket"), unwrap(argument))]
    greetd_socket: Option<String>,
    #[kdl(child(name = "max-users"), unwrap(argument))]
    max_users: Option<u64>,
    #[kdl(child(name = "max-auth-message-bytes"), unwrap(argument))]
    max_auth_message_bytes: Option<u64>,
    #[kdl(children(name = "session"))]
    sessions: Vec<SessionFile>,
}

#[derive(Debug, Decode)]
struct SessionFile {
    #[kdl(argument)]
    id: String,
    #[kdl(child, unwrap(argument))]
    label: String,
    #[kdl(child)]
    command: StringList,
    #[kdl(child)]
    environment: Option<StringList>,
}

#[derive(Debug, Decode)]
struct StringList {
    #[kdl(arguments)]
    values: Vec<String>,
}

impl FileConfig {
    fn resolve(self) -> Result<GreeterConfig, GreeterConfigError> {
        let defaults = GreeterConfig::default();
        let max_users = bounded("max-users", self.max_users, defaults.max_users, MAX_USERS)?;
        let max_auth_message_bytes = bounded(
            "max-auth-message-bytes",
            self.max_auth_message_bytes,
            defaults.max_auth_message_bytes,
            MAX_AUTH_MESSAGE_BYTES,
        )?;
        let sessions = if self.sessions.is_empty() {
            defaults.sessions
        } else {
            resolve_sessions(self.sessions)?
        };
        let greetd_socket = self
            .greetd_socket
            .map(PathBuf::from)
            .unwrap_or(defaults.greetd_socket);
        if greetd_socket.as_os_str().is_empty() {
            return Err(GreeterConfigError::EmptyGreetdSocket);
        }
        Ok(GreeterConfig {
            greetd_socket,
            sessions,
            max_users,
            max_auth_message_bytes,
        })
    }
}

fn resolve_sessions(
    sessions: Vec<SessionFile>,
) -> Result<Vec<SessionDefinition>, GreeterConfigError> {
    if sessions.len() > MAX_SESSIONS {
        return Err(GreeterConfigError::TooManySessions {
            count: sessions.len(),
        });
    }
    let mut ids = BTreeSet::new();
    sessions
        .into_iter()
        .map(|session| {
            if session.id.trim().is_empty() {
                return Err(GreeterConfigError::EmptySessionId);
            }
            if !ids.insert(session.id.clone()) {
                return Err(GreeterConfigError::DuplicateSession(session.id));
            }
            if session.label.trim().is_empty() {
                return Err(GreeterConfigError::EmptySessionLabel(session.id));
            }
            if session
                .command
                .values
                .first()
                .is_none_or(|value| value.is_empty())
            {
                return Err(GreeterConfigError::EmptySessionCommand(session.id));
            }
            let environment = session
                .environment
                .map(|list| list.values)
                .unwrap_or_default();
            for variable in &environment {
                let Some((name, _)) = variable.split_once('=') else {
                    return Err(GreeterConfigError::InvalidEnvironment {
                        session: session.id.clone(),
                        variable: variable.clone(),
                    });
                };
                if name.is_empty() || name.contains('\0') {
                    return Err(GreeterConfigError::InvalidEnvironment {
                        session: session.id.clone(),
                        variable: variable.clone(),
                    });
                }
            }
            Ok(SessionDefinition {
                id: session.id,
                label: session.label,
                command: session.command.values,
                environment,
            })
        })
        .collect()
}

fn bounded(
    field: &'static str,
    configured: Option<u64>,
    default: usize,
    maximum: usize,
) -> Result<usize, GreeterConfigError> {
    let value = configured.unwrap_or(default as u64);
    if value == 0 || value > maximum as u64 {
        return Err(GreeterConfigError::OutOfRange {
            field,
            value,
            maximum,
        });
    }
    Ok(value as usize)
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, thiserror::Error)]
pub enum GreeterConfigError {
    #[error("failed to read Tensor Greeter configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse Tensor Greeter configuration {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{field} must be in 1..={maximum}, got {value}")]
    OutOfRange {
        field: &'static str,
        value: u64,
        maximum: usize,
    },
    #[error("configured {count} sessions; at most {MAX_SESSIONS} are allowed")]
    TooManySessions { count: usize },
    #[error("session id must not be empty")]
    EmptySessionId,
    #[error("greetd-socket must not be empty")]
    EmptyGreetdSocket,
    #[error("session `{0}` is configured more than once")]
    DuplicateSession(String),
    #[error("session `{0}` has an empty label")]
    EmptySessionLabel(String),
    #[error("session `{0}` has an empty command")]
    EmptySessionCommand(String),
    #[error("session `{session}` has invalid environment entry `{variable}`")]
    InvalidEnvironment { session: String, variable: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(document: &str) -> Result<GreeterConfig, GreeterConfigError> {
        GreeterConfig::from_kdl(Path::new("greeter.kdl"), document)
    }

    #[test]
    fn typed_kdl_builds_an_argv_session_without_a_shell() {
        let config = parse(
            r#"
                greetd-socket "/tmp/greetd.sock"
                max-users 32
                session "tensorland" {
                    label "Tensorland"
                    command "tensor-session" "--example"
                    environment "XDG_SESSION_TYPE=wayland"
                }
            "#,
        )
        .unwrap();
        assert_eq!(config.max_users, 32);
        assert_eq!(config.sessions[0].command, ["tensor-session", "--example"]);
        assert_eq!(config.greetd_socket, Path::new("/tmp/greetd.sock"));
    }

    #[test]
    fn duplicate_sessions_and_shell_strings_are_rejected() {
        let duplicate = r#"
            session "tensorland" { label "One"; command "one"; }
            session "tensorland" { label "Two"; command "two"; }
        "#;
        assert!(matches!(
            parse(duplicate).unwrap_err(),
            GreeterConfigError::DuplicateSession(_)
        ));
        let invalid_env = r#"
            session "tensorland" {
                label "Tensorland"
                command "tensor-session"
                environment "DISPLAY"
            }
        "#;
        assert!(matches!(
            parse(invalid_env).unwrap_err(),
            GreeterConfigError::InvalidEnvironment { .. }
        ));
    }

    #[test]
    fn empty_greetd_socket_is_rejected_before_transport_setup() {
        assert!(matches!(
            parse("greetd-socket \"\""),
            Err(GreeterConfigError::EmptyGreetdSocket)
        ));
    }

    #[test]
    fn malformed_kdl_retains_the_named_source() {
        let error = parse("session {").unwrap_err().to_string();
        assert!(error.contains("greeter.kdl"));
        assert!(error.contains("failed to parse"));
    }
}
