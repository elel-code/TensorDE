use std::collections::BTreeMap;

use smithay::output::{Mode, Subpixel};

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
pub(crate) struct OutputPolicy;

impl OutputPolicy {
    pub(crate) fn plan<'a>(
        &self,
        connectors: impl IntoIterator<Item = &'a ConnectorSnapshot>,
    ) -> OutputPlan {
        connectors
            .into_iter()
            .filter_map(output_for_connector)
            .map(|output| (output.id, output))
            .collect()
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

fn output_for_connector(connector: &ConnectorSnapshot) -> Option<OutputDescriptor> {
    if connector.state != ConnectorState::Connected {
        return None;
    }
    Some(OutputDescriptor {
        id: connector.id,
        name: connector.name.clone(),
        physical_size: connector.physical_size,
        subpixel: connector.subpixel,
        modes: connector.modes.clone(),
        preferred_mode: connector.preferred_mode?,
        crtc: connector.mapped_crtc?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn policy_retains_only_connectors_ready_for_scanout() {
        let connected = connector(1, 1, ConnectorState::Connected, Some(7), Some(1920));
        let waiting_for_crtc = connector(1, 2, ConnectorState::Connected, None, Some(2560));
        let waiting_for_mode = connector(1, 3, ConnectorState::Connected, Some(8), None);
        let disconnected = connector(1, 4, ConnectorState::Disconnected, Some(9), Some(1280));

        let plan = OutputPolicy.plan([
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

        let ids = OutputPolicy
            .plan([&later_device, &later_connector, &first])
            .into_keys()
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![first.id, later_connector.id, later_device.id]);
    }

    #[test]
    fn plan_diff_disconnects_before_connecting_and_changing() {
        let removed = connector(1, 1, ConnectorState::Connected, Some(1), Some(1920));
        let old_changed = connector(1, 2, ConnectorState::Connected, Some(2), Some(1920));
        let changed = connector(1, 2, ConnectorState::Connected, Some(2), Some(2560));
        let added = connector(1, 3, ConnectorState::Connected, Some(3), Some(3840));
        let previous = OutputPolicy.plan([&removed, &old_changed]);
        let current = OutputPolicy.plan([&changed, &added]);

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
}
