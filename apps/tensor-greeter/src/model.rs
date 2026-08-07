use crate::{MAX_AUTH_MESSAGE_BYTES, MAX_SESSIONS, MAX_USERS, SessionDefinition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserAccount {
    pub username: String,
    pub display_name: String,
}

impl UserAccount {
    pub fn new(
        username: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, GreeterModelError> {
        let username = username.into();
        if username.is_empty() || username.contains('\0') {
            return Err(GreeterModelError::InvalidUsername);
        }
        Ok(Self {
            username,
            display_name: display_name.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AuthAttemptId(u64);

impl AuthAttemptId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthPromptKind {
    Visible,
    Secret,
    Info,
    Error,
}

impl AuthPromptKind {
    pub const fn requires_response(self) -> bool {
        matches!(self, Self::Visible | Self::Secret)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthPrompt {
    pub kind: AuthPromptKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthPhase {
    Idle,
    Waiting {
        attempt: AuthAttemptId,
    },
    Prompt {
        attempt: AuthAttemptId,
        prompt: AuthPrompt,
    },
    Authenticated {
        attempt: AuthAttemptId,
    },
    SessionStarted {
        attempt: AuthAttemptId,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthStart {
    pub attempt: AuthAttemptId,
    pub username: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionStart {
    pub attempt: AuthAttemptId,
    pub command: Vec<String>,
    pub environment: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GreeterModel {
    users: Vec<UserAccount>,
    sessions: Vec<SessionDefinition>,
    selected_user: Option<usize>,
    selected_session: usize,
    phase: AuthPhase,
    next_attempt: u64,
    max_auth_message_bytes: usize,
}

impl GreeterModel {
    pub fn new(
        users: Vec<UserAccount>,
        sessions: Vec<SessionDefinition>,
        max_auth_message_bytes: usize,
    ) -> Result<Self, GreeterModelError> {
        if users.len() > MAX_USERS {
            return Err(GreeterModelError::TooManyUsers(users.len()));
        }
        if sessions.is_empty() || sessions.len() > MAX_SESSIONS {
            return Err(GreeterModelError::InvalidSessionCount(sessions.len()));
        }
        if max_auth_message_bytes == 0 || max_auth_message_bytes > MAX_AUTH_MESSAGE_BYTES {
            return Err(GreeterModelError::InvalidMessageLimit(
                max_auth_message_bytes,
            ));
        }
        Ok(Self {
            selected_user: (!users.is_empty()).then_some(0),
            users,
            sessions,
            selected_session: 0,
            phase: AuthPhase::Idle,
            next_attempt: 1,
            max_auth_message_bytes,
        })
    }

    pub fn users(&self) -> &[UserAccount] {
        &self.users
    }

    pub fn sessions(&self) -> &[SessionDefinition] {
        &self.sessions
    }

    pub fn phase(&self) -> &AuthPhase {
        &self.phase
    }

    pub fn selected_user(&self) -> Option<&UserAccount> {
        self.selected_user.map(|index| &self.users[index])
    }

    pub fn selected_session(&self) -> &SessionDefinition {
        &self.sessions[self.selected_session]
    }

    pub const fn selected_session_index(&self) -> usize {
        self.selected_session
    }

    pub fn select_user(&mut self, index: usize) -> Result<(), GreeterModelError> {
        self.require_mutable_selection()?;
        if index >= self.users.len() {
            return Err(GreeterModelError::UnknownUser(index));
        }
        self.selected_user = Some(index);
        Ok(())
    }

    pub fn select_session(&mut self, index: usize) -> Result<(), GreeterModelError> {
        self.require_mutable_selection()?;
        if index >= self.sessions.len() {
            return Err(GreeterModelError::UnknownSession(index));
        }
        self.selected_session = index;
        Ok(())
    }

    pub fn begin_authentication(&mut self) -> Result<AuthStart, GreeterModelError> {
        self.require_mutable_selection()?;
        let username = self
            .selected_user()
            .ok_or(GreeterModelError::NoSelectedUser)?
            .username
            .clone();
        let attempt = AuthAttemptId(self.next_attempt);
        self.next_attempt = self.next_attempt.wrapping_add(1).max(1);
        self.phase = AuthPhase::Waiting { attempt };
        Ok(AuthStart { attempt, username })
    }

    pub fn receive_prompt(
        &mut self,
        attempt: AuthAttemptId,
        prompt: AuthPrompt,
    ) -> Result<(), GreeterModelError> {
        self.require_attempt(attempt)?;
        if prompt.message.len() > self.max_auth_message_bytes {
            return Err(GreeterModelError::AuthMessageTooLarge {
                bytes: prompt.message.len(),
                maximum: self.max_auth_message_bytes,
            });
        }
        self.phase = AuthPhase::Prompt { attempt, prompt };
        Ok(())
    }

    /// Advance after the caller encoded a response directly into a sensitive frame.
    ///
    /// The response itself is deliberately absent from this API, so the model can
    /// never retain a password, PIN, or visible challenge answer.
    pub fn response_sent(
        &mut self,
        attempt: AuthAttemptId,
        had_response: bool,
    ) -> Result<(), GreeterModelError> {
        let AuthPhase::Prompt {
            attempt: current,
            prompt,
        } = &self.phase
        else {
            return Err(GreeterModelError::InvalidTransition("response"));
        };
        if *current != attempt {
            return Err(GreeterModelError::StaleAttempt(attempt));
        }
        if prompt.kind.requires_response() != had_response {
            return Err(GreeterModelError::UnexpectedPromptResponse);
        }
        self.phase = AuthPhase::Waiting { attempt };
        Ok(())
    }

    pub fn authentication_succeeded(
        &mut self,
        attempt: AuthAttemptId,
    ) -> Result<(), GreeterModelError> {
        self.require_attempt(attempt)?;
        self.phase = AuthPhase::Authenticated { attempt };
        Ok(())
    }

    pub fn authentication_failed(
        &mut self,
        attempt: AuthAttemptId,
        message: impl Into<String>,
    ) -> Result<(), GreeterModelError> {
        self.require_attempt(attempt)?;
        self.phase = AuthPhase::Failed {
            message: message.into(),
        };
        Ok(())
    }

    pub fn start_session(&self, attempt: AuthAttemptId) -> Result<SessionStart, GreeterModelError> {
        match self.phase {
            AuthPhase::Authenticated { attempt: current } if current == attempt => {
                let session = self.selected_session();
                Ok(SessionStart {
                    attempt,
                    command: session.command.clone(),
                    environment: session.environment.clone(),
                })
            }
            AuthPhase::Authenticated { .. } => Err(GreeterModelError::StaleAttempt(attempt)),
            _ => Err(GreeterModelError::InvalidTransition("start-session")),
        }
    }

    pub fn session_started(&mut self, attempt: AuthAttemptId) -> Result<(), GreeterModelError> {
        match self.phase {
            AuthPhase::Authenticated { attempt: current } if current == attempt => {
                self.phase = AuthPhase::SessionStarted { attempt };
                Ok(())
            }
            AuthPhase::Authenticated { .. } => Err(GreeterModelError::StaleAttempt(attempt)),
            _ => Err(GreeterModelError::InvalidTransition("session-started")),
        }
    }

    pub fn cancel(&mut self) {
        self.phase = AuthPhase::Idle;
    }

    fn require_mutable_selection(&self) -> Result<(), GreeterModelError> {
        match self.phase {
            AuthPhase::Idle | AuthPhase::Failed { .. } => Ok(()),
            _ => Err(GreeterModelError::SelectionLocked),
        }
    }

    fn require_attempt(&self, attempt: AuthAttemptId) -> Result<(), GreeterModelError> {
        let current = match self.phase {
            AuthPhase::Waiting { attempt }
            | AuthPhase::Prompt { attempt, .. }
            | AuthPhase::Authenticated { attempt } => attempt,
            AuthPhase::Idle | AuthPhase::Failed { .. } | AuthPhase::SessionStarted { .. } => {
                return Err(GreeterModelError::InvalidTransition(
                    "authentication-result",
                ));
            }
        };
        if current != attempt {
            return Err(GreeterModelError::StaleAttempt(attempt));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GreeterModelError {
    #[error("username must be non-empty and contain no NUL")]
    InvalidUsername,
    #[error("configured {0} users; at most {MAX_USERS} are allowed")]
    TooManyUsers(usize),
    #[error("configured {0} sessions; expected 1..={MAX_SESSIONS}")]
    InvalidSessionCount(usize),
    #[error("authentication message limit {0} is outside 1..={MAX_AUTH_MESSAGE_BYTES}")]
    InvalidMessageLimit(usize),
    #[error("user index {0} does not exist")]
    UnknownUser(usize),
    #[error("session index {0} does not exist")]
    UnknownSession(usize),
    #[error("no user is selected")]
    NoSelectedUser,
    #[error("user and session selection is locked during authentication")]
    SelectionLocked,
    #[error("stale authentication attempt {}", .0.get())]
    StaleAttempt(AuthAttemptId),
    #[error("cannot apply {0} in the current authentication phase")]
    InvalidTransition(&'static str),
    #[error("authentication prompt response presence does not match its kind")]
    UnexpectedPromptResponse,
    #[error("authentication message has {bytes} bytes; maximum is {maximum}")]
    AuthMessageTooLarge { bytes: usize, maximum: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionDefinition {
        SessionDefinition {
            id: "tensorland".into(),
            label: "Tensorland".into(),
            command: vec!["tensor-session".into()],
            environment: vec!["XDG_SESSION_TYPE=wayland".into()],
        }
    }

    fn model() -> GreeterModel {
        GreeterModel::new(
            vec![UserAccount::new("tensor", "Tensor User").unwrap()],
            vec![session()],
            1024,
        )
        .unwrap()
    }

    #[test]
    fn authentication_never_accepts_a_secret_into_retained_state() {
        let mut model = model();
        let start = model.begin_authentication().unwrap();
        model
            .receive_prompt(
                start.attempt,
                AuthPrompt {
                    kind: AuthPromptKind::Secret,
                    message: "Password:".into(),
                },
            )
            .unwrap();
        model.response_sent(start.attempt, true).unwrap();
        assert_eq!(
            model.phase(),
            &AuthPhase::Waiting {
                attempt: start.attempt
            }
        );
        assert!(!format!("{model:?}").contains("hunter2"));
    }

    #[test]
    fn stale_results_cannot_authenticate_a_new_attempt() {
        let mut model = model();
        let first = model.begin_authentication().unwrap();
        model.cancel();
        let second = model.begin_authentication().unwrap();
        assert_ne!(first.attempt, second.attempt);
        assert!(matches!(
            model.authentication_succeeded(first.attempt),
            Err(GreeterModelError::StaleAttempt(_))
        ));
    }

    #[test]
    fn session_starts_only_after_the_matching_success() {
        let mut model = model();
        let start = model.begin_authentication().unwrap();
        assert!(model.start_session(start.attempt).is_err());
        model.authentication_succeeded(start.attempt).unwrap();
        let session = model.start_session(start.attempt).unwrap();
        assert_eq!(session.command, ["tensor-session"]);
        model.session_started(start.attempt).unwrap();
        assert!(model.start_session(start.attempt).is_err());
        assert!(model.select_session(0).is_err());
    }

    #[test]
    fn session_selection_is_available_before_authentication() {
        let mut model = GreeterModel::new(
            vec![UserAccount::new("tensor", "Tensor User").unwrap()],
            vec![
                session(),
                SessionDefinition {
                    id: "safe".into(),
                    label: "Safe Session".into(),
                    command: vec!["safe-session".into()],
                    environment: Vec::new(),
                },
            ],
            1024,
        )
        .unwrap();
        assert_eq!(model.selected_session_index(), 0);
        model.select_session(1).unwrap();
        assert_eq!(model.selected_session_index(), 1);
        assert_eq!(model.selected_session().id, "safe");
        let attempt = model.begin_authentication().unwrap();
        assert!(model.start_session(attempt.attempt).is_err());
    }

    #[test]
    fn informational_prompts_require_an_empty_response() {
        let mut model = model();
        let start = model.begin_authentication().unwrap();
        model
            .receive_prompt(
                start.attempt,
                AuthPrompt {
                    kind: AuthPromptKind::Info,
                    message: "Touch the security key".into(),
                },
            )
            .unwrap();
        assert!(matches!(
            model.response_sent(start.attempt, true),
            Err(GreeterModelError::UnexpectedPromptResponse)
        ));
        model.response_sent(start.attempt, false).unwrap();
    }
}
