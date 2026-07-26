use super::{LibinputEvent, TtyBackend, UdevEvent};

impl TtyBackend {
    pub(crate) fn drain_udev_completions(&mut self) -> Result<Vec<UdevEvent>, String> {
        let mut events = Vec::new();
        while let Some(completion) = self.udev_completions.try_recv() {
            events.extend(self.udev.drain());
            completion
                .rearm()
                .map_err(|error| format!("udev completion rearm was rejected: {error:?}"))?;
        }
        if let Some(message) = self.udev_failures.try_recv() {
            return Err(message);
        }
        Ok(events)
    }

    pub(crate) fn drain_libinput_completions(&mut self) -> Result<Vec<LibinputEvent>, String> {
        let mut events = Vec::new();
        while let Some(completion) = self.libinput_completions.try_recv() {
            match self.libinput.drain() {
                Ok(completed) => events.extend(completed),
                Err(error) => {
                    let _ = completion.finish();
                    return Err(format!(
                        "failed to dispatch completed libinput events: {error}"
                    ));
                }
            }
            completion
                .rearm()
                .map_err(|error| format!("libinput completion rearm was rejected: {error:?}"))?;
        }
        if let Some(message) = self.libinput_failures.try_recv() {
            return Err(message);
        }
        Ok(events)
    }
}
