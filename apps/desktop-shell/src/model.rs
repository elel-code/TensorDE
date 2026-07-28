use std::collections::{BTreeMap, BTreeSet};

use wayland_client_runtime::{OutputEvent, OutputId, OutputInfo};

use crate::{ShellLayout, SurfacePlan, surface_plan};

/// Independently managed products exposed by the desktop shell.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShellComponent {
    Panel,
    Launcher,
    Notifications,
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
                self.outputs.insert(output, info);
                self.visible.insert((output, ShellComponent::Panel));
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
}
