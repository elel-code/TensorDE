#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanEffectDebugBbox {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    initialized: bool,
}

impl Default for NativeVulkanEffectDebugBbox {
    fn default() -> Self {
        Self {
            min_x: u32::MAX,
            min_y: u32::MAX,
            max_x: 0,
            max_y: 0,
            initialized: false,
        }
    }
}

impl NativeVulkanEffectDebugBbox {
    pub(super) fn include(&mut self, x: u32, y: u32) {
        if !self.initialized {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
            self.initialized = true;
            return;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    pub(super) fn label(self) -> String {
        if !self.initialized {
            return "none".to_owned();
        }
        format!(
            "{}..{}x{}..{}",
            self.min_x, self.max_x, self.min_y, self.max_y
        )
    }
}
