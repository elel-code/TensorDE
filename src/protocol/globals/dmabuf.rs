use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use wayland_server::DisplayHandle;

use super::super::state::RuntimeState;

/// Owns the linux-dmabuf protocol state and its immutable default feedback.
///
/// The global is created only after Vulkan has supplied a format list that the
/// renderer can actually import. Keeping the delegate alive even when no
/// eligible format exists lets the handler reject unexpected requests without
/// advertising an unusable global.
pub(crate) struct DmabufProtocol {
    pub(crate) state: DmabufState,
    pub(crate) global: Option<DmabufGlobal>,
    feedback: Option<DmabufFeedback>,
}

impl DmabufProtocol {
    pub(crate) fn new() -> Self {
        Self {
            state: DmabufState::new(),
            global: None,
            feedback: None,
        }
    }

    pub(crate) fn install(
        &mut self,
        display: &DisplayHandle,
        main_device: libc::dev_t,
        formats: impl IntoIterator<Item = tensor_host::DrmFormat>,
    ) -> Result<bool, String> {
        let formats = formats
            .into_iter()
            .map(crate::backend::smithay_drm_format)
            .collect::<Vec<_>>();
        if formats.is_empty() {
            return Ok(false);
        }
        let feedback = DmabufFeedbackBuilder::new(main_device, formats)
            .build()
            .map_err(|error| error.to_string())?;
        let global = self
            .state
            .create_global_with_default_feedback::<RuntimeState>(display, &feedback);
        self.global = Some(global);
        self.feedback = Some(feedback);
        Ok(true)
    }

    pub(crate) fn advertised(&self) -> bool {
        self.global.is_some() && self.feedback.is_some()
    }
}

#[cfg(test)]
mod tests {
    use tensor_host::{DrmFormat, Fourcc, Modifier};
    use wayland_server::Display;

    use super::*;

    #[test]
    fn feedback_global_is_created_only_for_a_nonempty_import_contract() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut protocol = DmabufProtocol::new();
        assert!(
            !protocol
                .install(&display.handle(), 0, std::iter::empty())
                .unwrap()
        );
        assert!(!protocol.advertised());

        let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
        assert!(protocol.install(&display.handle(), 0, [format]).unwrap());
        assert!(protocol.advertised());
    }
}
