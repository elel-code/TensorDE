//! Build and diff output plans from connector snapshots + rules.

use std::collections::BTreeMap;

use tensor_host::{ConnectorId, ConnectorState};
use tensor_util::Size;

use crate::{
    policy::{OutputRuleTable, guess_monitor_scale, select_mode},
    snapshot::{ConnectorSnapshot, OutputDescriptor},
};

/// Planned outputs keyed by connector identity (stable BTree order).
pub type OutputPlan = BTreeMap<ConnectorId, OutputDescriptor>;

/// Topology change emitted when reconciling plans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanEvent {
    Connected(OutputDescriptor),
    Changed(OutputDescriptor),
    Disconnected(ConnectorId),
}

/// Diff result: events in a deterministic order (disconnects first, then activate).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutputPlanDiff {
    pub events: Vec<PlanEvent>,
}

/// Plan which connectors should be active given rules.
pub fn plan_outputs(rules: &OutputRuleTable, connectors: &[ConnectorSnapshot]) -> OutputPlan {
    let mut plan = OutputPlan::new();
    for connector in connectors {
        if let Some(descriptor) = plan_one(rules, connector) {
            plan.insert(descriptor.id, descriptor);
        }
    }
    plan
}

fn plan_one(rules: &OutputRuleTable, connector: &ConnectorSnapshot) -> Option<OutputDescriptor> {
    if connector.state != ConnectorState::Connected || connector.non_desktop {
        return None;
    }
    let rule = rules.get(&connector.name);
    if rule.is_some_and(|r| !r.enabled) {
        return None;
    }
    if !connector.has_native_format {
        return None;
    }
    let preferred = connector.preferred_mode?;
    let mode = select_mode(&connector.modes, preferred, rule)?;
    let resolution = Size::new(
        u32::try_from(mode.width).ok()?,
        u32::try_from(mode.height).ok()?,
    );
    let scale = rule
        .and_then(|r| r.scale)
        .unwrap_or_else(|| guess_monitor_scale(connector.physical_size_mm, resolution));
    Some(OutputDescriptor {
        id: connector.id,
        name: connector.name.clone(),
        physical_size_mm: connector.physical_size_mm,
        subpixel: connector.subpixel,
        modes: connector.modes.clone(),
        mode,
        crtc: connector.mapped_crtc?,
        scale,
        position: rule.and_then(|r| r.position),
        has_native_format: true,
    })
}

/// Diff two plans into value-only topology events.
pub fn diff_plans(previous: &OutputPlan, current: &OutputPlan) -> OutputPlanDiff {
    let disconnected = previous
        .keys()
        .filter(|id| !current.contains_key(id))
        .copied()
        .map(PlanEvent::Disconnected);
    let activated = current
        .iter()
        .filter_map(|(id, descriptor)| match previous.get(id) {
            None => Some(PlanEvent::Connected(descriptor.clone())),
            Some(old) if old != descriptor => Some(PlanEvent::Changed(descriptor.clone())),
            Some(_) => None,
        });
    OutputPlanDiff {
        events: disconnected.chain(activated).collect(),
    }
}

#[cfg(test)]
mod tests {
    use tensor_host::{ConnectorId, ConnectorState, PhysicalMode, SubpixelLayout};

    use super::*;
    use crate::policy::OutputRule;

    fn snap(
        device: u64,
        conn: u32,
        state: ConnectorState,
        crtc: Option<u32>,
        width: Option<i32>,
        has_fmt: bool,
    ) -> ConnectorSnapshot {
        let mode = width.map(|w| PhysicalMode::new(w, 1080, 60_000));
        ConnectorSnapshot {
            id: ConnectorId::new(device, conn),
            name: format!("card-{device}-connector-{conn}"),
            state,
            non_desktop: false,
            physical_size_mm: (600, 340),
            subpixel: SubpixelLayout::HorizontalRgb,
            modes: mode.into_iter().collect(),
            preferred_mode: mode,
            mapped_crtc: crtc,
            has_native_format: has_fmt,
        }
    }

    #[test]
    fn plan_keeps_only_scanout_ready_connectors() {
        let connectors = [
            snap(1, 1, ConnectorState::Connected, Some(7), Some(1920), true),
            snap(1, 2, ConnectorState::Connected, None, Some(2560), true),
            snap(1, 3, ConnectorState::Connected, Some(8), None, true),
            snap(
                1,
                4,
                ConnectorState::Disconnected,
                Some(9),
                Some(1280),
                true,
            ),
            snap(1, 5, ConnectorState::Connected, Some(10), Some(1280), false),
        ];
        let plan = plan_outputs(&OutputRuleTable::default(), &connectors);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[&ConnectorId::new(1, 1)].crtc, 7);
    }

    #[test]
    fn plan_never_claims_non_desktop_connectors() {
        let mut connector = snap(1, 1, ConnectorState::Connected, Some(7), Some(1920), true);
        connector.non_desktop = true;

        assert!(plan_outputs(&OutputRuleTable::default(), &[connector]).is_empty());
    }

    #[test]
    fn disabled_rule_drops_connector() {
        let mut rules = OutputRuleTable::default();
        rules.upsert(
            "card-1-connector-1",
            OutputRule {
                enabled: false,
                ..OutputRule::default()
            },
        );
        let connectors = [snap(
            1,
            1,
            ConnectorState::Connected,
            Some(7),
            Some(1920),
            true,
        )];
        assert!(plan_outputs(&rules, &connectors).is_empty());
    }

    #[test]
    fn plan_order_is_stable_by_connector_id() {
        let connectors = [
            snap(9, 1, ConnectorState::Connected, Some(1), Some(1920), true),
            snap(2, 8, ConnectorState::Connected, Some(2), Some(1920), true),
            snap(2, 3, ConnectorState::Connected, Some(3), Some(1920), true),
        ];
        let ids: Vec<_> = plan_outputs(&OutputRuleTable::default(), &connectors)
            .into_keys()
            .collect();
        assert_eq!(
            ids,
            vec![
                ConnectorId::new(2, 3),
                ConnectorId::new(2, 8),
                ConnectorId::new(9, 1)
            ]
        );
    }

    #[test]
    fn diff_emits_disconnect_then_connect() {
        let a = snap(1, 1, ConnectorState::Connected, Some(1), Some(1920), true);
        let b = snap(1, 2, ConnectorState::Connected, Some(2), Some(1920), true);
        let prev = plan_outputs(&OutputRuleTable::default(), std::slice::from_ref(&a));
        let next = plan_outputs(&OutputRuleTable::default(), &[b]);
        let diff = diff_plans(&prev, &next);
        assert!(matches!(diff.events[0], PlanEvent::Disconnected(_)));
        assert!(matches!(diff.events[1], PlanEvent::Connected(_)));
    }
}
