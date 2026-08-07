use tensor_dbus::{
    Connection,
    freedesktop::login1::{self, Login1Error},
};

use crate::{IdleAction, IdlePlan, IdleTransition};

/// Async login1 action endpoint driven entirely by the caller's Compio runtime.
pub struct LogindActionExecutor {
    connection: Connection,
}

impl LogindActionExecutor {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub async fn connect() -> Result<Self, SystemActionError> {
        Ok(Self::new(Connection::system_bus().await?))
    }

    /// Applies one idle transition. Monitor power remains a Wayland action;
    /// resume transitions never repeat one-shot lock or suspend requests.
    pub async fn apply_transition(
        &mut self,
        transition: IdleTransition,
    ) -> Result<bool, SystemActionError> {
        apply_transition(self, transition).await
    }
}

pub fn system_actions_required(plan: &IdlePlan) -> bool {
    plan.enabled
        && plan
            .stages
            .iter()
            .any(|stage| matches!(stage.action, IdleAction::Lock | IdleAction::Suspend))
}

trait SystemActionProtocol {
    async fn lock_sessions(&mut self) -> Result<(), SystemActionError>;
    async fn suspend(&mut self) -> Result<(), SystemActionError>;
}

impl SystemActionProtocol for LogindActionExecutor {
    async fn lock_sessions(&mut self) -> Result<(), SystemActionError> {
        Ok(login1::lock_sessions(&mut self.connection).await?)
    }

    async fn suspend(&mut self) -> Result<(), SystemActionError> {
        Ok(login1::suspend(&mut self.connection, false).await?)
    }
}

async fn apply_transition(
    protocol: &mut impl SystemActionProtocol,
    transition: IdleTransition,
) -> Result<bool, SystemActionError> {
    if !transition.idle {
        return Ok(false);
    }
    match transition.action {
        IdleAction::MonitorOff => Ok(false),
        IdleAction::Lock => {
            protocol.lock_sessions().await?;
            Ok(true)
        }
        IdleAction::Suspend => {
            protocol.suspend().await?;
            Ok(true)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemActionError {
    #[error(transparent)]
    Dbus(#[from] tensor_dbus::Error),
    #[error(transparent)]
    Login1(#[from] Login1Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdleConfig, PowerSource};

    #[derive(Default)]
    struct FakeProtocol {
        locks: usize,
        suspends: usize,
    }

    impl SystemActionProtocol for FakeProtocol {
        async fn lock_sessions(&mut self) -> Result<(), SystemActionError> {
            self.locks += 1;
            Ok(())
        }

        async fn suspend(&mut self) -> Result<(), SystemActionError> {
            self.suspends += 1;
            Ok(())
        }
    }

    fn transition(action: IdleAction, idle: bool) -> IdleTransition {
        IdleTransition {
            after_ms: 1,
            action,
            idle,
        }
    }

    #[test]
    fn plan_requests_a_system_connection_only_for_enabled_system_actions() {
        let plan = IdlePlan::compile(&IdleConfig::default(), PowerSource::Ac);
        assert!(system_actions_required(&plan));

        let config = IdleConfig {
            enabled: false,
            ..IdleConfig::default()
        };
        assert!(!system_actions_required(&IdlePlan::compile(
            &config,
            PowerSource::Ac
        )));
    }

    #[test]
    fn only_idle_edges_execute_one_shot_logind_actions() {
        tensor_runtime::io_uring_runtime(16)
            .unwrap()
            .block_on(async {
                let mut protocol = FakeProtocol::default();
                assert!(
                    apply_transition(&mut protocol, transition(IdleAction::Lock, true))
                        .await
                        .unwrap()
                );
                assert!(
                    !apply_transition(&mut protocol, transition(IdleAction::Lock, false))
                        .await
                        .unwrap()
                );
                assert!(
                    apply_transition(&mut protocol, transition(IdleAction::Suspend, true))
                        .await
                        .unwrap()
                );
                assert!(
                    !apply_transition(&mut protocol, transition(IdleAction::MonitorOff, true))
                        .await
                        .unwrap()
                );
                assert_eq!(protocol.locks, 1);
                assert_eq!(protocol.suspends, 1);
            });
    }
}
