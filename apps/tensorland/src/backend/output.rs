//! Output topology policy — compositor wrapper over `tensor-drm`.
//!
//! Types are Smithay-free (`tensor_host` / `tensor_drm`). The tty adapter maps
//! DRM scan results into these values; protocol maps them into `wl_output`.

use std::collections::BTreeMap;

use tensor_drm::{
    ConnectorSnapshot as DrmConnectorSnapshot, OutputDescriptor as DrmOutputDescriptor,
    OutputPlan as DrmOutputPlan, OutputRule as DrmOutputRule, OutputRuleTable, PlanEvent,
    diff_plans, plan_outputs,
};
use tensor_host::{ConnectorId, ConnectorState, PhysicalMode, SubpixelLayout};
use tensor_util::OutputScale;

use crate::{
    config::{OutputMode, OutputRule},
    render::OutputFormat,
};

// Scale heuristic lives in `tensor_drm::guess_monitor_scale` (Smithay-free).

/// Backend output identity (alias of host [`ConnectorId`]).
pub(crate) type BackendOutputId = ConnectorId;

/// Discovered connector after DRM scan (+ negotiated format for this compositor).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectorSnapshot {
    pub(crate) id: BackendOutputId,
    pub(crate) name: String,
    pub(crate) state: ConnectorState,
    pub(crate) physical_size: (i32, i32),
    pub(crate) subpixel: SubpixelLayout,
    pub(crate) modes: Vec<PhysicalMode>,
    pub(crate) preferred_mode: Option<PhysicalMode>,
    pub(crate) mapped_crtc: Option<u32>,
    pub(crate) native_format: Option<OutputFormat>,
}

/// Planned scanout target for one connector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OutputDescriptor {
    pub(crate) id: BackendOutputId,
    pub(crate) name: String,
    pub(crate) physical_size: (i32, i32),
    pub(crate) subpixel: SubpixelLayout,
    pub(crate) modes: Vec<PhysicalMode>,
    pub(crate) mode: PhysicalMode,
    pub(crate) crtc: u32,
    pub(crate) native_format: OutputFormat,
    pub(crate) scale: OutputScale,
    pub(crate) position: Option<(i32, i32)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BackendOutputEvent {
    Connected(OutputDescriptor),
    Changed(OutputDescriptor),
    Disconnected(BackendOutputId),
}

pub(crate) type OutputPlan = BTreeMap<BackendOutputId, OutputDescriptor>;

#[derive(Debug, Default)]
pub(crate) struct OutputPolicy {
    rules: OutputRuleTable,
}

impl OutputPolicy {
    pub(crate) fn new(configured_rules: BTreeMap<String, OutputRule>) -> Self {
        let mut rules = BTreeMap::new();
        for (name, rule) in configured_rules {
            rules.insert(name, to_drm_rule(rule));
        }
        Self {
            rules: OutputRuleTable::new(rules),
        }
    }

    pub(crate) fn rules(&self) -> BTreeMap<String, OutputRule> {
        self.rules
            .rules()
            .iter()
            .map(|(k, v)| (k.clone(), from_drm_rule(v)))
            .collect()
    }

    /// Insert or update one connector rule by name.
    pub(crate) fn upsert_rule(&mut self, name: impl Into<String>, rule: OutputRule) {
        self.rules.upsert(name, to_drm_rule(rule));
    }

    pub(crate) fn plan<'a>(
        &self,
        connectors: impl IntoIterator<Item = &'a ConnectorSnapshot>,
    ) -> OutputPlan {
        let connectors: Vec<&ConnectorSnapshot> = connectors.into_iter().collect();
        let snaps: Vec<_> = connectors.iter().map(|c| to_drm_snapshot(c)).collect();
        let planned = plan_outputs(&self.rules, &snaps);
        let mut out = OutputPlan::new();
        for (id, desc) in planned {
            let Some(snap) = connectors.iter().find(|c| c.id == id) else {
                continue;
            };
            let Some(native_format) = snap.native_format else {
                continue;
            };
            out.insert(id, from_drm_descriptor(desc, native_format));
        }
        out
    }
}

fn to_drm_rule(rule: OutputRule) -> DrmOutputRule {
    DrmOutputRule {
        scale: rule.scale,
        mode: rule.mode.map(|m| tensor_drm::OutputModeRequest {
            width: m.width,
            height: m.height,
            refresh_millihertz: m.refresh_millihertz,
        }),
        position: rule.position,
        enabled: rule.enabled,
        max_refresh_millihertz: rule.max_refresh_millihertz,
    }
}

fn from_drm_rule(rule: &DrmOutputRule) -> OutputRule {
    OutputRule {
        scale: rule.scale,
        mode: rule.mode.map(|m| OutputMode {
            width: m.width,
            height: m.height,
            refresh_millihertz: m.refresh_millihertz,
        }),
        position: rule.position,
        enabled: rule.enabled,
        max_refresh_millihertz: rule.max_refresh_millihertz,
    }
}

fn to_drm_snapshot(c: &ConnectorSnapshot) -> DrmConnectorSnapshot {
    DrmConnectorSnapshot {
        id: c.id,
        name: c.name.clone(),
        state: c.state,
        physical_size_mm: c.physical_size,
        subpixel: c.subpixel,
        modes: c.modes.clone(),
        preferred_mode: c.preferred_mode,
        mapped_crtc: c.mapped_crtc,
        has_native_format: c.native_format.is_some(),
    }
}

fn from_drm_descriptor(desc: DrmOutputDescriptor, native_format: OutputFormat) -> OutputDescriptor {
    OutputDescriptor {
        id: desc.id,
        name: desc.name,
        physical_size: desc.physical_size_mm,
        subpixel: desc.subpixel,
        modes: desc.modes,
        mode: desc.mode,
        crtc: desc.crtc,
        native_format,
        scale: desc.scale,
        position: desc.position,
    }
}

fn to_drm_desc_for_diff(d: &OutputDescriptor) -> DrmOutputDescriptor {
    DrmOutputDescriptor {
        id: d.id,
        name: d.name.clone(),
        physical_size_mm: d.physical_size,
        subpixel: d.subpixel,
        modes: d.modes.clone(),
        mode: d.mode,
        crtc: d.crtc,
        scale: d.scale,
        position: d.position,
        has_native_format: true,
    }
}

pub(crate) fn diff_output_plans(
    previous: &OutputPlan,
    current: &OutputPlan,
) -> Vec<BackendOutputEvent> {
    let prev: DrmOutputPlan = previous
        .iter()
        .map(|(id, d)| (*id, to_drm_desc_for_diff(d)))
        .collect();
    let curr: DrmOutputPlan = current
        .iter()
        .map(|(id, d)| (*id, to_drm_desc_for_diff(d)))
        .collect();
    let diff = diff_plans(&prev, &curr);
    diff.events
        .into_iter()
        .filter_map(|event| match event {
            PlanEvent::Connected(d) => {
                let fmt = current.get(&d.id)?.native_format;
                Some(BackendOutputEvent::Connected(from_drm_descriptor(d, fmt)))
            }
            PlanEvent::Changed(d) => {
                let fmt = current.get(&d.id)?.native_format;
                Some(BackendOutputEvent::Changed(from_drm_descriptor(d, fmt)))
            }
            PlanEvent::Disconnected(id) => Some(BackendOutputEvent::Disconnected(id)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tensor_host::{DrmFormat, Fourcc, Modifier};

    use super::*;

    fn native_format(modifier: u64) -> OutputFormat {
        OutputFormat {
            format: DrmFormat {
                code: Fourcc::XRGB8888,
                modifier: Modifier::from_raw(modifier),
            },
            plane_count: 1,
        }
    }

    fn connector(
        device_id: u64,
        connector_id: u32,
        state: ConnectorState,
        mapped_crtc: Option<u32>,
        width: Option<i32>,
    ) -> ConnectorSnapshot {
        let mode = width.map(|width| PhysicalMode::new(width, 1080, 60_000));
        ConnectorSnapshot {
            id: BackendOutputId::new(device_id, connector_id),
            name: format!("card-{device_id}-connector-{connector_id}"),
            state,
            physical_size: (600, 340),
            subpixel: SubpixelLayout::HorizontalRgb,
            modes: mode.into_iter().collect(),
            preferred_mode: mode,
            mapped_crtc,
            native_format: Some(native_format(9)),
        }
    }

    #[test]
    fn policy_retains_only_connectors_ready_for_scanout() {
        let connected = connector(1, 1, ConnectorState::Connected, Some(7), Some(1920));
        let waiting_for_crtc = connector(1, 2, ConnectorState::Connected, None, Some(2560));
        let waiting_for_mode = connector(1, 3, ConnectorState::Connected, Some(8), None);
        let disconnected = connector(1, 4, ConnectorState::Disconnected, Some(9), Some(1280));

        let plan = OutputPolicy::default().plan([
            &disconnected,
            &waiting_for_mode,
            &waiting_for_crtc,
            &connected,
        ]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[&connected.id].crtc, 7);
    }

    #[test]
    fn plan_is_stably_ordered_across_devices() {
        let later_device = connector(9, 1, ConnectorState::Connected, Some(1), Some(1920));
        let later_connector = connector(2, 8, ConnectorState::Connected, Some(2), Some(1920));
        let first = connector(2, 3, ConnectorState::Connected, Some(3), Some(1920));

        let ids = OutputPolicy::default()
            .plan([&later_device, &later_connector, &first])
            .into_keys()
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![first.id, later_connector.id, later_device.id]);
    }

    #[test]
    fn policy_waits_for_native_format_negotiation() {
        let mut connector = connector(1, 1, ConnectorState::Connected, Some(7), Some(1920));
        connector.native_format = None;

        assert!(OutputPolicy::default().plan([&connector]).is_empty());
    }

    #[test]
    fn configured_mode_selects_matching_refresh() {
        let mut snap = connector(1, 1, ConnectorState::Connected, Some(1), Some(1920));
        snap.modes = vec![
            PhysicalMode::new(1920, 1080, 60_000),
            PhysicalMode::new(1920, 1080, 144_000),
        ];
        snap.preferred_mode = Some(PhysicalMode::new(1920, 1080, 60_000));

        let mut rules = BTreeMap::new();
        rules.insert(
            snap.name.clone(),
            OutputRule {
                mode: Some(OutputMode {
                    width: 1920,
                    height: 1080,
                    refresh_millihertz: Some(144_000),
                }),
                ..OutputRule::new()
            },
        );
        let plan = OutputPolicy::new(rules).plan([&snap]);
        assert_eq!(plan[&snap.id].mode.refresh_millihertz, 144_000);
    }

    #[test]
    fn diff_reports_disconnect_and_connect() {
        let a = connector(1, 1, ConnectorState::Connected, Some(1), Some(1920));
        let b = connector(1, 2, ConnectorState::Connected, Some(2), Some(1920));
        let prev = OutputPolicy::default().plan([&a]);
        let next = OutputPolicy::default().plan([&b]);
        let events = diff_output_plans(&prev, &next);
        assert!(matches!(
            events[0],
            BackendOutputEvent::Disconnected(id) if id == a.id
        ));
        assert!(matches!(
            events[1],
            BackendOutputEvent::Connected(ref d) if d.id == b.id
        ));
    }
}
