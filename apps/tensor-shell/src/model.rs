use std::collections::{BTreeMap, BTreeSet};

use wayland_client_runtime::{OutputEvent, OutputId, OutputInfo};

use crate::{ShellLayout, SurfaceKey, SurfacePlan, surface_plan};

/// Independently managed products exposed by the desktop shell.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShellComponent {
    Panel,
    Launcher,
    NotificationCenter,
    NotificationPopups,
    Osd,
    ControlCenter,
    Overview,
    LockScreen,
}

/// Output and visibility state, kept independent from Wayland object handles.
#[derive(Debug)]
pub struct ShellModel {
    layout: ShellLayout,
    outputs: BTreeMap<OutputId, OutputInfo>,
    visible: BTreeSet<(OutputId, ShellComponent)>,
}

impl ShellModel {
    pub fn new(layout: ShellLayout) -> Self {
        Self {
            layout,
            outputs: BTreeMap::new(),
            visible: BTreeSet::new(),
        }
    }

    pub fn apply_output_event(&mut self, event: OutputEvent) {
        match event {
            OutputEvent::Added(info) | OutputEvent::Updated(info) => {
                let output = info.id;
                let was_known = self.outputs.insert(output, info).is_some();
                if !was_known {
                    self.visible.insert((output, ShellComponent::Panel));
                }
            }
            OutputEvent::Removed(output) => {
                self.outputs.remove(&output);
                self.visible.retain(|(candidate, _)| *candidate != output);
            }
        }
    }

    pub fn set_visible(&mut self, output: OutputId, component: ShellComponent, visible: bool) {
        let key = (output, component);
        if visible && self.outputs.contains_key(&output) {
            if component.is_interactive() {
                self.visible.retain(|(candidate, current)| {
                    *candidate != output || !current.is_interactive() || *current == component
                });
            }
            self.visible.insert(key);
        } else {
            self.visible.remove(&key);
        }
    }

    pub fn plans(&self) -> impl Iterator<Item = SurfacePlan> + '_ {
        self.visible
            .iter()
            .copied()
            .map(|(output, component)| surface_plan(component, output, self.layout))
    }

    pub fn visible(&self, output: OutputId, component: ShellComponent) -> bool {
        self.visible.contains(&(output, component))
    }

    pub fn surface_keys(&self) -> impl Iterator<Item = SurfaceKey> + '_ {
        self.visible
            .iter()
            .copied()
            .map(|(output, component)| SurfaceKey { output, component })
    }

    pub fn output_ids(&self) -> impl Iterator<Item = OutputId> + '_ {
        self.outputs.keys().copied()
    }
}

impl ShellComponent {
    pub const fn is_interactive(self) -> bool {
        matches!(
            self,
            Self::Launcher
                | Self::NotificationCenter
                | Self::ControlCenter
                | Self::Overview
                | Self::LockScreen
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: u32) -> OutputInfo {
        OutputInfo {
            id: OutputId::from_raw(id),
            name: Some(format!("OUT-{id}")),
            description: None,
            make: "Tensor".into(),
            model: "Virtual".into(),
            logical_position: None,
            logical_size: None,
            scale_factor: 1,
            refresh_mhz: None,
        }
    }

    #[test]
    fn updated_output_does_not_resurrect_a_hidden_panel() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::Panel, false);
        model.apply_output_event(OutputEvent::Updated(output(1)));
        assert!(!model.visible(id, ShellComponent::Panel));
    }

    #[test]
    fn duplicate_added_event_does_not_resurrect_a_hidden_panel() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::Panel, false);
        model.apply_output_event(OutputEvent::Added(output(1)));
        assert!(!model.visible(id, ShellComponent::Panel));
    }

    #[test]
    fn interactive_surfaces_are_exclusive_per_output() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::Launcher, true);
        model.set_visible(id, ShellComponent::ControlCenter, true);
        assert!(!model.visible(id, ShellComponent::Launcher));
        assert!(model.visible(id, ShellComponent::ControlCenter));
    }

    #[test]
    fn removing_output_removes_every_surface_key() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::NotificationPopups, true);
        model.apply_output_event(OutputEvent::Removed(id));
        assert_eq!(model.surface_keys().count(), 0);
    }

    #[test]
    fn notification_center_replaces_other_interactive_surfaces() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::Launcher, true);
        model.set_visible(id, ShellComponent::NotificationCenter, true);
        assert!(!model.visible(id, ShellComponent::Launcher));
        assert!(model.visible(id, ShellComponent::NotificationCenter));
    }

    #[test]
    fn notification_popups_do_not_displace_an_interactive_surface() {
        let id = OutputId::from_raw(1);
        let mut model = ShellModel::new(ShellLayout::default());
        model.apply_output_event(OutputEvent::Added(output(1)));
        model.set_visible(id, ShellComponent::ControlCenter, true);
        model.set_visible(id, ShellComponent::NotificationPopups, true);
        assert!(model.visible(id, ShellComponent::ControlCenter));
        assert!(model.visible(id, ShellComponent::NotificationPopups));
    }
}
