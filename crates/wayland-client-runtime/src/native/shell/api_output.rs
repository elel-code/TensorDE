//! Output inventory helpers on [`NativeShell`].

use super::api::NativeShell;

impl NativeShell {
    /// Snapshot of currently known outputs into `out` (clears first, reuses capacity).
    pub fn outputs_into(&self, out: &mut Vec<crate::output::OutputInfo>) {
        out.clear();
        out.reserve(self.state.outputs.len());
        for &name in self.state.outputs.keys() {
            if let Some(info) = self.output_info(name) {
                out.push(info);
            }
        }
        out.sort_by_key(|o| o.id.get());
    }

    /// Snapshot of currently known outputs (allocates a new `Vec`).
    ///
    /// Prefer [`Self::outputs_into`] in hot loops to reuse capacity.
    pub fn outputs(&self) -> Vec<crate::output::OutputInfo> {
        let mut list = Vec::with_capacity(self.state.outputs.len());
        self.outputs_into(&mut list);
        list
    }

    /// Single-output snapshot by registry global name (`OutputId::get()`).
    pub fn output_info(&self, name: u32) -> Option<crate::output::OutputInfo> {
        use crate::geometry::{LogicalPosition, LogicalSize};
        use crate::output::{OutputId, OutputInfo};
        let rec = self.state.outputs.get(&name)?;
        Some(OutputInfo {
            id: OutputId::from_raw(name),
            name: rec.name.clone(),
            description: rec.description.clone(),
            make: rec.make.clone(),
            model: rec.model.clone(),
            logical_position: Some(LogicalPosition::new(rec.x, rec.y)),
            logical_size: if rec.mode_width > 0 && rec.mode_height > 0 {
                Some(LogicalSize::new(
                    rec.mode_width as u32,
                    rec.mode_height as u32,
                ))
            } else if rec.physical_width > 0 && rec.physical_height > 0 {
                Some(LogicalSize::new(
                    rec.physical_width as u32,
                    rec.physical_height as u32,
                ))
            } else {
                None
            },
            scale_factor: rec.scale,
            refresh_mhz: (rec.mode_refresh_mhz > 0).then_some(rec.mode_refresh_mhz),
        })
    }


    pub fn output_scale_factor(&self, output_name: u32) -> Option<i32> {
        self.state.outputs.get(&output_name).map(|o| o.scale)
    }

    /// Find an output by compositor-advertised name (`wl_output.name`, v4+).
    ///
    /// Comparison is case-sensitive and exact. Useful for binding a layer
    /// surface to a specific monitor (Gilder `output_name` option).
    pub fn find_output_by_name(&self, name: &str) -> Option<crate::output::OutputInfo> {
        self.state
            .outputs
            .iter()
            .find(|(_, rec)| rec.name.as_deref() == Some(name))
            .and_then(|(&global_name, _)| self.output_info(global_name))
    }
}
