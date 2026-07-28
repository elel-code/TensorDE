/// Logical desktop-shell dimensions. Compositors scale these per output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellLayout {
    pub panel_height: u32,
    pub popover_width: u32,
    pub popover_height: u32,
    pub osd_width: u32,
    pub osd_height: u32,
    pub edge_gap: i32,
}

impl Default for ShellLayout {
    fn default() -> Self {
        Self {
            panel_height: 40,
            popover_width: 420,
            popover_height: 560,
            osd_width: 320,
            osd_height: 96,
            edge_gap: 12,
        }
    }
}

impl ShellLayout {
    pub fn validate(self) -> Result<Self, ShellLayoutError> {
        if self.panel_height == 0 {
            return Err(ShellLayoutError::ZeroPanelHeight);
        }
        if self.popover_width == 0 || self.popover_height == 0 {
            return Err(ShellLayoutError::EmptyPopover);
        }
        if self.osd_width == 0 || self.osd_height == 0 {
            return Err(ShellLayoutError::EmptyOsd);
        }
        if self.edge_gap < 0 {
            return Err(ShellLayoutError::NegativeEdgeGap);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ShellLayoutError {
    #[error("panel height must be non-zero")]
    ZeroPanelHeight,
    #[error("popover dimensions must be non-zero")]
    EmptyPopover,
    #[error("OSD dimensions must be non-zero")]
    EmptyOsd,
    #[error("edge gap must not be negative")]
    NegativeEdgeGap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_is_valid() {
        assert_eq!(
            ShellLayout::default().validate(),
            Ok(ShellLayout::default())
        );
    }

    #[test]
    fn zero_panel_height_is_rejected() {
        let layout = ShellLayout {
            panel_height: 0,
            ..ShellLayout::default()
        };
        assert_eq!(layout.validate(), Err(ShellLayoutError::ZeroPanelHeight));
    }
}
