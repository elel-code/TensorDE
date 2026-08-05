//! Runtime output policy mutations (IPC / control plane / wlr-output-management).

use super::RuntimeState;
use crate::protocol::extensions::output_management::HeadSnapshot;

impl RuntimeState {
    pub(crate) fn apply_output_rule(
        &mut self,
        name: String,
        mutate: impl FnOnce(&mut crate::config::OutputRule),
    ) -> Result<(), String> {
        #[cfg(feature = "tty")]
        {
            let Some(backend) = self.backend.as_mut() else {
                return Err("tty backend is not available".to_owned());
            };
            let mut rule = backend.output_rules().remove(&name).unwrap_or_default();
            mutate(&mut rule);
            backend.upsert_output_rule(name, rule);
            let events = backend.take_output_events();
            self.apply_backend_output_events(events)
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = (name, mutate);
            Err("output reconfiguration requires the tty backend".to_owned())
        }
    }

    pub(crate) fn apply_output_configuration(
        &mut self,
        updates: Vec<(String, tensor_protocol::OutputHeadUpdate)>,
    ) -> Result<(), String> {
        #[cfg(feature = "tty")]
        {
            let Some(backend) = self.backend.as_mut() else {
                return Err("tty backend is not available".to_owned());
            };
            let mut rules = backend.output_rules();
            for (name, update) in updates {
                let rule = rules.entry(name).or_default();
                if let Some(enabled) = update.enabled {
                    rule.enabled = enabled;
                }
                if let Some(position) = update.position {
                    rule.position = Some(position);
                }
                if let Some(scale) = update.scale {
                    rule.scale = Some(scale);
                }
            }
            backend.replace_output_rules(rules);
            let events = backend.take_output_events();
            self.apply_backend_output_events(events)
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = updates;
            Err("output reconfiguration requires the tty backend".to_owned())
        }
    }

    /// Value-only head list for `zwlr_output_management_v1` advertisement.
    ///
    /// Includes policy-disabled connectors so clients can re-enable them.
    /// Geometry for active heads comes from WindowSpace; disabled heads report 0,0.
    pub(crate) fn output_management_heads(&self) -> Vec<HeadSnapshot> {
        #[cfg(feature = "tty")]
        {
            use std::collections::BTreeMap;

            let mut by_name: BTreeMap<String, HeadSnapshot> = BTreeMap::new();
            if let Some(backend) = self.backend.as_ref() {
                let rules = backend.output_rules();
                for (name, mw, mh, refresh, enabled) in backend.management_connector_heads() {
                    let rule = rules.get(&name);
                    let scale = rule.and_then(|r| r.scale).unwrap_or_default();
                    let position = rule.and_then(|r| r.position).unwrap_or((0, 0));
                    by_name.insert(
                        name.clone(),
                        HeadSnapshot {
                            name,
                            x: position.0,
                            y: position.1,
                            width: mw,
                            height: mh,
                            scale,
                            mode_width: mw,
                            mode_height: mh,
                            refresh_millihertz: refresh,
                            enabled,
                        },
                    );
                }
            }
            // Overlay live WindowSpace geometry / scale for currently mapped outputs.
            for managed in self.outputs.values() {
                let d = &managed.descriptor;
                let loc = self
                    .space
                    .output_geometry(&managed.output)
                    .map(|geo| (geo.loc.x, geo.loc.y))
                    .unwrap_or((0, 0));
                by_name.insert(
                    d.name.clone(),
                    HeadSnapshot {
                        name: d.name.clone(),
                        x: loc.0,
                        y: loc.1,
                        width: d.mode.width,
                        height: d.mode.height,
                        scale: d.scale,
                        mode_width: d.mode.width,
                        mode_height: d.mode.height,
                        refresh_millihertz: d.mode.refresh_millihertz,
                        enabled: true,
                    },
                );
            }
            by_name.into_values().collect()
        }
        #[cfg(not(feature = "tty"))]
        {
            Vec::new()
        }
    }

    /// Re-advertise topology to bound output-management clients (topology rate only).
    pub(crate) fn refresh_output_management_protocol(&mut self) {
        let heads = self.output_management_heads();
        self.protocol_globals
            .output_management()
            .notify_heads::<RuntimeState>(heads);
    }
}
