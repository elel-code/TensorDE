//! Compositor-side commit of completed configuration candidates.

use tensor_runtime::WorkerRx;
use tracing::{info, warn};

use crate::{
    config::{
        ConfigReloadOutcome, ConfigReloadResult, ConfigTransaction,
        MAX_PENDING_CONFIG_RELOAD_RESULTS,
    },
    ipc::{ConfigReloadEvent, ConfigReloadEventResult, EventTopic, IpcSubscriptions, ServerEvent},
    protocol::RuntimeState,
};

pub(super) fn drain_config_reload_outcomes(
    outcomes: &WorkerRx<ConfigReloadOutcome>,
    transaction: &mut ConfigTransaction,
    state: &mut RuntimeState,
    subscriptions: &mut IpcSubscriptions,
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
        let event = match result {
            ConfigReloadResult::Applied { generation } => {
                info!(request_id, generation, "configuration reload applied");
                ConfigReloadEvent {
                    request_id,
                    generation,
                    result: ConfigReloadEventResult::Applied,
                }
            }
            ConfigReloadResult::Rejected(failure) => {
                warn!(
                    request_id,
                    generation = failure.generation,
                    error_code = failure.diagnostic.error_code,
                    error = %failure.error,
                    "configuration reload rejected"
                );
                ConfigReloadEvent {
                    request_id,
                    generation: failure.generation,
                    result: ConfigReloadEventResult::Rejected {
                        diagnostic: failure.diagnostic,
                    },
                }
            }
        };
        let summary =
            subscriptions.publish(EventTopic::ConfigReload, ServerEvent::ConfigReload(event));
        if summary.dropped != 0 {
            warn!(
                dropped = summary.dropped,
                "slow IPC configuration subscribers were disconnected"
            );
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

        let mut subscriptions = IpcSubscriptions::new();
        let (subscription, mut events) =
            crate::ipc::subscription_channel(vec![EventTopic::ConfigReload]);
        subscriptions.register(subscription).unwrap();

        drain_config_reload_outcomes(&outcomes, &mut transaction, &mut state, &mut subscriptions);

        assert_eq!(transaction.generation(), 1);
        assert_eq!(state.layout.kind(), LayoutKind::Spatial2D);
        let applied = events.try_recv().unwrap();
        assert_eq!(
            applied.event,
            ServerEvent::ConfigReload(ConfigReloadEvent {
                request_id: 1,
                generation: 1,
                result: ConfigReloadEventResult::Applied,
            })
        );

        let mut restart_required = transaction.active().clone();
        restart_required.ipc_socket = PathBuf::from("/tmp/tensor-replaced.sock");
        sender
            .try_send(ConfigReloadOutcome {
                request_id: 2,
                candidate: Ok(restart_required),
            })
            .unwrap();

        drain_config_reload_outcomes(&outcomes, &mut transaction, &mut state, &mut subscriptions);

        assert_eq!(transaction.generation(), 1);
        assert_eq!(state.layout.kind(), LayoutKind::Spatial2D);
        assert_eq!(
            transaction
                .last_failure()
                .map(|failure| failure.error_code.as_str()),
            Some("reload_requires_restart")
        );
        let rejected = events.try_recv().unwrap();
        let ServerEvent::ConfigReload(rejected) = rejected.event;
        assert_eq!(rejected.request_id, 2);
        assert_eq!(rejected.generation, 1);
        assert!(matches!(
            rejected.result,
            ConfigReloadEventResult::Rejected { .. }
        ));
    }
}
