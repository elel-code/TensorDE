impl ShellScene {
    fn toggle_background_blur(&mut self) -> bool {
        self.background_blur = !self.background_blur;
        self.appearance_changes += 1;
        true
    }

    fn set_background_opacity_percent(&mut self, percent: u8) -> bool {
        let percent = background_opacity_percent(percent as f32 / 100.0);
        let opacity = percent as f32 / 100.0;
        if (self.background_opacity - opacity).abs() <= f32::EPSILON {
            return false;
        }
        self.background_opacity = opacity;
        self.appearance_changes += 1;
        true
    }
}
