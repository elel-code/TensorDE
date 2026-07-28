use std::collections::BTreeMap;

use wayland_client_runtime::{
    Event, LayerSurfaceEvent, OutputEvent, OutputId, Runtime, RuntimeError, SurfaceId,
};

use crate::{ShellComponent, ShellLayout, surface_plan};

/// Protocol runtime for the first shell slice: one GPU panel per output.
pub struct ShellRuntime {
    wayland: Runtime,
    layout: ShellLayout,
    panels: BTreeMap<OutputId, SurfaceId>,
    events: Vec<Event>,
}

impl ShellRuntime {
    pub fn connect() -> Result<Self, RuntimeError> {
        let wayland = Runtime::connect()?;
        if !wayland.capabilities().layer_shell_v1 {
            return Err(RuntimeError::Unsupported("layer-shell-v1"));
        }
        Ok(Self {
            wayland,
            layout: ShellLayout::default(),
            panels: BTreeMap::new(),
            events: Vec::with_capacity(128),
        })
    }

    pub fn run(mut self) -> Result<(), RuntimeError> {
        loop {
            self.wayland.dispatch(None)?;
            self.events.clear();
            self.wayland.drain_events_into(&mut self.events);
            let events = std::mem::take(&mut self.events);
            for event in &events {
                self.handle_event(event)?;
            }
            self.events = events;
        }
    }

    fn handle_event(&mut self, event: &Event) -> Result<(), RuntimeError> {
        match event {
            Event::Output(OutputEvent::Added(info) | OutputEvent::Updated(info)) => {
                self.ensure_panel(info.id)?;
            }
            Event::Output(OutputEvent::Removed(output)) => self.remove_panel(*output)?,
            Event::LayerSurface(LayerSurfaceEvent::Closed { surface }) => {
                self.panels.retain(|_, candidate| candidate != surface);
            }
            _ => {}
        }
        Ok(())
    }

    fn ensure_panel(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        if self.panels.contains_key(&output) {
            return Ok(());
        }
        let plan = surface_plan(ShellComponent::Panel, output, self.layout);
        let surface = self.wayland.create_layer_surface_gpu(plan.attributes)?;
        self.panels.insert(output, surface);
        Ok(())
    }

    fn remove_panel(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        if let Some(surface) = self.panels.remove(&output) {
            self.wayland.destroy_surface(surface)?;
        }
        Ok(())
    }
}
