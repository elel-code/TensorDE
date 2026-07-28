use crate::{FikaWgpuApp, save_background_effect_settings};

impl FikaWgpuApp {
    pub(crate) fn toggle_user_background_blur(&mut self) -> bool {
        if !self.scene.toggle_background_blur() {
            return false;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_blur(self.scene.background_blur);
        }
        self.save_background_effect_settings();
        true
    }

    pub(crate) fn set_user_background_opacity_percent(&mut self, percent: u8) -> bool {
        if !self.scene.set_background_opacity_percent(percent) {
            return false;
        }
        self.save_background_effect_settings();
        true
    }

    fn save_background_effect_settings(&self) {
        if let Err(error) = save_background_effect_settings(
            &self.settings_path,
            self.scene.background_blur,
            self.scene.background_opacity,
        ) {
            fika_log!("[fika-wgpu] settings-save-error {error}");
        }
    }
}
