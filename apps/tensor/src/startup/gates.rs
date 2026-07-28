use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupMode {
    Check,
    Compositor,
    Session { systemd_active: bool },
}

#[derive(Debug)]
pub(crate) struct StartupGates {
    mode: StartupMode,
    runtime_prepared: bool,
    process_environment_published: bool,
    manager_environment_published: bool,
    readiness_published: bool,
    autostart_authorized: bool,
}

#[derive(Debug)]
pub(crate) struct SessionAutostartPermit {
    _private: (),
}

impl StartupGates {
    pub(crate) const fn new(check: bool, session: bool, systemd_active: bool) -> Self {
        let mode = if check {
            StartupMode::Check
        } else if session {
            StartupMode::Session { systemd_active }
        } else {
            StartupMode::Compositor
        };
        Self {
            mode,
            runtime_prepared: false,
            process_environment_published: false,
            manager_environment_published: false,
            readiness_published: false,
            autostart_authorized: false,
        }
    }

    pub(crate) fn mark_runtime_prepared(&mut self) {
        self.runtime_prepared = true;
    }

    pub(crate) fn mark_process_environment_published(&mut self) -> Result<(), StartupGateError> {
        self.require_runtime()?;
        if !matches!(self.mode, StartupMode::Session { .. }) {
            return Err(StartupGateError::SessionEnvironmentNotRequired);
        }
        self.process_environment_published = true;
        Ok(())
    }

    #[cfg(any(feature = "systemd", test))]
    pub(crate) fn mark_manager_environment_published(&mut self) -> Result<(), StartupGateError> {
        self.require_runtime()?;
        if !self.process_environment_published {
            return Err(StartupGateError::ProcessEnvironmentNotPublished);
        }
        if !matches!(
            self.mode,
            StartupMode::Session {
                systemd_active: true
            }
        ) {
            return Err(StartupGateError::ManagerEnvironmentNotRequired);
        }
        self.manager_environment_published = true;
        Ok(())
    }

    pub(crate) fn mark_readiness_published(&mut self) -> Result<(), StartupGateError> {
        self.require_runtime()?;
        if matches!(self.mode, StartupMode::Session { .. }) && !self.process_environment_published {
            return Err(StartupGateError::ProcessEnvironmentNotPublished);
        }
        if matches!(
            self.mode,
            StartupMode::Session {
                systemd_active: true
            }
        ) && !self.manager_environment_published
        {
            return Err(StartupGateError::ManagerEnvironmentNotPublished);
        }
        self.readiness_published = true;
        Ok(())
    }

    pub(crate) fn authorize_autostart(
        &mut self,
    ) -> Result<Option<SessionAutostartPermit>, StartupGateError> {
        let StartupMode::Session { systemd_active } = self.mode else {
            return Ok(None);
        };
        self.require_runtime()?;
        if !self.process_environment_published {
            return Err(StartupGateError::ProcessEnvironmentNotPublished);
        }
        if systemd_active && !self.manager_environment_published {
            return Err(StartupGateError::ManagerEnvironmentNotPublished);
        }
        if !self.readiness_published {
            return Err(StartupGateError::ReadinessNotPublished);
        }
        if self.autostart_authorized {
            return Err(StartupGateError::AutostartAlreadyAuthorized);
        }
        self.autostart_authorized = true;
        Ok(Some(SessionAutostartPermit { _private: () }))
    }

    fn require_runtime(&self) -> Result<(), StartupGateError> {
        if self.runtime_prepared {
            Ok(())
        } else {
            Err(StartupGateError::RuntimeNotPrepared)
        }
    }
}

#[derive(Debug, Eq, Error, PartialEq)]
pub enum StartupGateError {
    #[error("the compositor runtime is not prepared")]
    RuntimeNotPrepared,
    #[error("the session process environment is not published")]
    ProcessEnvironmentNotPublished,
    #[error("the systemd user-manager environment is not published")]
    ManagerEnvironmentNotPublished,
    #[error("startup readiness is not published")]
    ReadinessNotPublished,
    #[error("session autostart was already authorized")]
    AutostartAlreadyAuthorized,
    #[error("session environment publication is not valid for this startup mode")]
    SessionEnvironmentNotRequired,
    #[error("systemd user-manager environment publication is not required")]
    #[cfg(any(feature = "systemd", test))]
    ManagerEnvironmentNotRequired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_and_non_session_modes_never_authorize_autostart() {
        assert!(
            StartupGates::new(true, true, true)
                .authorize_autostart()
                .unwrap()
                .is_none()
        );
        assert!(
            StartupGates::new(false, false, false)
                .authorize_autostart()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn direct_session_requires_runtime_environment_and_readiness() {
        let mut gates = StartupGates::new(false, true, false);
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::RuntimeNotPrepared
        );

        gates.mark_runtime_prepared();
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::ProcessEnvironmentNotPublished
        );
        gates.mark_process_environment_published().unwrap();
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::ReadinessNotPublished
        );

        gates.mark_readiness_published().unwrap();
        assert!(gates.authorize_autostart().unwrap().is_some());
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::AutostartAlreadyAuthorized
        );
    }

    #[test]
    fn systemd_session_requires_manager_environment_before_readiness() {
        let mut gates = StartupGates::new(false, true, true);
        gates.mark_runtime_prepared();
        gates.mark_process_environment_published().unwrap();

        assert_eq!(
            gates.mark_readiness_published().unwrap_err(),
            StartupGateError::ManagerEnvironmentNotPublished
        );
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::ManagerEnvironmentNotPublished
        );

        gates.mark_manager_environment_published().unwrap();
        assert_eq!(
            gates.authorize_autostart().unwrap_err(),
            StartupGateError::ReadinessNotPublished
        );
        gates.mark_readiness_published().unwrap();
        assert!(gates.authorize_autostart().unwrap().is_some());
    }

    #[test]
    fn environment_transitions_cannot_run_ahead_of_runtime() {
        let mut gates = StartupGates::new(false, true, true);
        assert_eq!(
            gates.mark_process_environment_published().unwrap_err(),
            StartupGateError::RuntimeNotPrepared
        );
        assert_eq!(
            gates.mark_manager_environment_published().unwrap_err(),
            StartupGateError::RuntimeNotPrepared
        );
        assert_eq!(
            gates.mark_readiness_published().unwrap_err(),
            StartupGateError::RuntimeNotPrepared
        );
    }

    #[test]
    fn environment_transitions_reject_the_wrong_session_mode() {
        let mut compositor = StartupGates::new(false, false, false);
        compositor.mark_runtime_prepared();
        assert_eq!(
            compositor.mark_process_environment_published().unwrap_err(),
            StartupGateError::SessionEnvironmentNotRequired
        );

        let mut direct_session = StartupGates::new(false, true, false);
        direct_session.mark_runtime_prepared();
        direct_session.mark_process_environment_published().unwrap();
        assert_eq!(
            direct_session
                .mark_manager_environment_published()
                .unwrap_err(),
            StartupGateError::ManagerEnvironmentNotRequired
        );
    }
}
