//! Compositor-side commit of completed configuration candidates.

use tensor_runtime::WorkerRx;
use tracing::{info, warn};

use crate::{
    config::{
        ConfigReloadOutcome, ConfigReloadResult, ConfigTransaction,
        MAX_PENDING_CONFIG_RELOAD_RESULTS,
    },
    protocol::RuntimeState,
};

pub(super) fn drain_config_reload_outcomes(
    outcomes: &WorkerRx<ConfigReloadOutcome>,
    transaction: &mut ConfigTransaction,
    state: &mut RuntimeState,
) {
    outcomes.drain(MAX_PENDING_CONFIG_RELOAD_RESULTS, |outcome| {
        let request_id = outcome.request_id;
        let result = match outcome.candidate {
            Ok(candidate) => {
                if let Some(field) = transaction.active().restart_required_change(&candidate) {
                    transaction.reject_restart_required(field)
                } else {
                    state.apply_live_config(&candidate);
                    transaction.apply_candidate(Ok(candidate))
                }
            }
            Err(error) => transaction.apply_candidate(Err(error)),
        };
        match result {
            ConfigReloadResult::Applied { generation } => {
                info!(request_id, generation, "configuration reload applied");
            }
            ConfigReloadResult::Rejected(failure) => {
                warn!(
                    request_id,
                    generation = failure.generation,
                    error_code = failure.diagnostic.error_code,
                    error = %failure.error,
                    "configuration reload rejected"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tensor_runtime::WorkerBridge;

    use super::*;
    use crate::{
        config::Config,
        layout::{LayoutEngine, LayoutKind},
        protocol::test_runtime_state,
        scene::SceneAppearance,
    };

    #[test]
    fn completed_reload_commits_live_policy_and_rejects_static_changes() {
        let initial = Config::default();
        let mut transaction = ConfigTransaction::new("test.kdl", initial.clone());
        let mut state = test_runtime_state(
            LayoutEngine::new(initial.initial_layout),
            SceneAppearance::default(),
        );
        let (sender, outcomes) = WorkerBridge::bounded(MAX_PENDING_CONFIG_RELOAD_RESULTS);
        let mut live = initial.clone();
        live.initial_layout = LayoutKind::Spatial2D;
        sender
            .try_send(ConfigReloadOutcome {
                request_id: 1,
                candidate: Ok(live),
            })
            .unwrap();

        drain_config_reload_outcomes(&outcomes, &mut transaction, &mut state);

        assert_eq!(transaction.generation(), 1);
        assert_eq!(state.layout.kind(), LayoutKind::Spatial2D);

        let mut restart_required = transaction.active().clone();
        restart_required.ipc_socket = PathBuf::from("/tmp/tensor-replaced.sock");
        sender
            .try_send(ConfigReloadOutcome {
                request_id: 2,
                candidate: Ok(restart_required),
            })
            .unwrap();

        drain_config_reload_outcomes(&outcomes, &mut transaction, &mut state);

        assert_eq!(transaction.generation(), 1);
        assert_eq!(state.layout.kind(), LayoutKind::Spatial2D);
        assert_eq!(
            transaction
                .last_failure()
                .map(|failure| failure.error_code.as_str()),
            Some("reload_requires_restart")
        );
    }
}
