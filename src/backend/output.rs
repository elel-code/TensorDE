use std::collections::BTreeMap;

use smithay::output::{Mode, Subpixel};
use tensor_util::{OutputScale, Size};

use crate::render::OutputFormat;

mod scale;
use scale::guess_monitor_scale;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectorSnapshot {
    pub(crate) id: BackendOutputId,
    pub(crate) name: String,
    pub(crate) state: ConnectorState,
    pub(crate) physical_size: (i32, i32),
    pub(crate) subpixel: Subpixel,
    pub(crate) modes: Vec<Mode>,
    pub(crate) preferred_mode: Option<Mode>,
    pub(crate) mapped_crtc: Option<u32>,
    pub(crate) native_format: Option<OutputFormat>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectorState {
    Connected,
    Disconnected,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OutputDescriptor {
    pub(crate) id: BackendOutputId,
    pub(crate) name: String,
    pub(crate) physical_size: (i32, i32),
    pub(crate) subpixel: Subpixel,
    pub(crate) modes: Vec<Mode>,
    pub(crate) preferred_mode: Mode,
    pub(crate) crtc: u32,
    pub(crate) native_format: OutputFormat,
    pub(crate) scale: OutputScale,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct BackendOutputId {
    pub(crate) device_id: u64,
    pub(crate) connector_id: u32,
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
    configured_scales: BTreeMap<String, OutputScale>,
}

impl OutputPolicy {
    pub(crate) fn new(configured_scales: BTreeMap<String, OutputScale>) -> Self {
        Self { configured_scales }
    }

    pub(crate) fn plan<'a>(
        &self,
        connectors: impl IntoIterator<Item = &'a ConnectorSnapshot>,
    ) -> OutputPlan {
        connectors
            .into_iter()
            .filter_map(|connector| self.output_for_connector(connector))
            .map(|output| (output.id, output))
            .collect()
    }

    fn output_for_connector(&self, connector: &ConnectorSnapshot) -> Option<OutputDescriptor> {
        if connector.state != ConnectorState::Connected {
            return None;
        }
        let preferred_mode = connector.preferred_mode?;
        let resolution = Size::new(
            u32::try_from(preferred_mode.size.w).ok()?,
            u32::try_from(preferred_mode.size.h).ok()?,
        );
        let scale = self
            .configured_scales
            .get(&connector.name)
            .copied()
            .unwrap_or_else(|| guess_monitor_scale(connector.physical_size, resolution));
        Some(OutputDescriptor {
            id: connector.id,
            name: connector.name.clone(),
            physical_size: connector.physical_size,
            subpixel: connector.subpixel,
            modes: connector.modes.clone(),
            preferred_mode,
            crtc: connector.mapped_crtc?,
            native_format: connector.native_format?,
            scale,
        })
    }
}

pub(crate) fn diff_output_plans(
    previous: &OutputPlan,
    current: &OutputPlan,
) -> Vec<BackendOutputEvent> {
    let disconnected = previous
        .keys()
        .filter(|id| !current.contains_key(id))
        .copied()
        .map(BackendOutputEvent::Disconnected);
    let activated = current
        .iter()
        .filter_map(|(id, descriptor)| match previous.get(id) {
            None => Some(BackendOutputEvent::Connected(descriptor.clone())),
            Some(old) if old != descriptor => Some(BackendOutputEvent::Changed(descriptor.clone())),
            Some(_) => None,
        });
    disconnected.chain(activated).collect()
}

#[cfg(test)]
mod tests {
    use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};

    use super::*;

    fn native_format(modifier: u64) -> OutputFormat {
        OutputFormat {
            format: DrmFormat {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::from(modifier),
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
        let mode = width.map(|width| Mode {
            size: (width, 1080).into(),
            refresh: 60_000,
        });
        ConnectorSnapshot {
            id: BackendOutputId {
                device_id,
                connector_id,
            },
            name: format!("card-{device_id}-connector-{connector_id}"),
            state,
            physical_size: (600, 340),
            subpixel: Subpixel::HorizontalRgb,
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
    fn plan_diff_disconnects_before_connecting_and_changing() {
        let removed = connector(1, 1, ConnectorState::Connected, Some(1), Some(1920));
        let old_changed = connector(1, 2, ConnectorState::Connected, Some(2), Some(1920));
        let changed = connector(1, 2, ConnectorState::Connected, Some(2), Some(2560));
        let added = connector(1, 3, ConnectorState::Connected, Some(3), Some(3840));
        let previous = OutputPolicy::default().plan([&removed, &old_changed]);
        let current = OutputPolicy::default().plan([&changed, &added]);

        let events = diff_output_plans(&previous, &current);

        assert_eq!(
            events,
            vec![
                BackendOutputEvent::Disconnected(removed.id),
                BackendOutputEvent::Changed(current[&changed.id].clone()),
                BackendOutputEvent::Connected(current[&added.id].clone()),
            ]
        );
    }

    #[test]
    fn format_change_is_an_output_change() {
        let old = connector(1, 1, ConnectorState::Connected, Some(1), Some(1920));
        let mut changed = old.clone();
        changed.native_format = Some(native_format(10));
        let previous = OutputPolicy::default().plan([&old]);
        let current = OutputPolicy::default().plan([&changed]);

        assert_eq!(
            diff_output_plans(&previous, &current),
            vec![BackendOutputEvent::Changed(current[&changed.id].clone())]
        );
    }

    #[test]
    fn configured_scale_overrides_the_monitor_heuristic() {
        let connector = connector(1, 1, ConnectorState::Connected, Some(7), Some(1920));
        let policy = OutputPolicy::new(
            [(connector.name.clone(), OutputScale::from_f64(1.25).unwrap())]
                .into_iter()
                .collect(),
        );
        assert_eq!(
            policy.plan([&connector])[&connector.id].scale,
            OutputScale::from_f64(1.25).unwrap()
        );
    }
}
