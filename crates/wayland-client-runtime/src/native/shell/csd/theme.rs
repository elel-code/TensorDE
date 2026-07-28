//! Color themes for client-side decorations (Adwaita-inspired).

/// ARGB color as premultiplied-friendly components `[a, r, g, b]`.
pub type Argb = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorMap {
    pub headerbar: Argb,
    pub button_idle: Argb,
    pub button_hover: Argb,
    pub button_icon: Argb,
    pub border: Argb,
    pub title: Argb,
    pub edge: Argb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorTheme {
    pub active: ColorMap,
    pub inactive: ColorMap,
}

impl ColorTheme {
    #[allow(dead_code)]
    pub const fn light() -> Self {
        Self {
            active: ColorMap {
                headerbar: [0xff, 0xeb, 0xeb, 0xeb],
                button_idle: [0xff, 0xd8, 0xd8, 0xd8],
                button_hover: [0xff, 0xcf, 0xcf, 0xcf],
                button_icon: [0xff, 0x2a, 0x2a, 0x2a],
                border: [0xff, 0xdc, 0xdc, 0xdc],
                title: [0xff, 0x2f, 0x2f, 0x2f],
                edge: [0xff, 0xb0, 0xb0, 0xb0],
            },
            inactive: ColorMap {
                headerbar: [0xff, 0xfa, 0xfa, 0xfa],
                button_idle: [0xff, 0xf0, 0xf0, 0xf0],
                button_hover: [0xff, 0xd8, 0xd8, 0xd8],
                button_icon: [0xff, 0x94, 0x94, 0x94],
                border: [0xff, 0xdc, 0xdc, 0xdc],
                title: [0xff, 0x96, 0x96, 0x96],
                edge: [0xff, 0xc8, 0xc8, 0xc8],
            },
        }
    }

    pub const fn dark() -> Self {
        Self {
            active: ColorMap {
                headerbar: [0xff, 0x32, 0x2e, 0x2e],
                button_idle: [0xff, 0x47, 0x43, 0x43],
                button_hover: [0xff, 0x4f, 0x4f, 0x4f],
                button_icon: [0xff, 0xff, 0xff, 0xff],
                border: [0xff, 0x3a, 0x3a, 0x3a],
                title: [0xff, 0xff, 0xff, 0xff],
                edge: [0xff, 0x28, 0x28, 0x28],
            },
            inactive: ColorMap {
                headerbar: [0xff, 0x26, 0x22, 0x22],
                button_idle: [0xff, 0x31, 0x2d, 0x2d],
                button_hover: [0xff, 0x39, 0x39, 0x39],
                button_icon: [0xff, 0x90, 0x90, 0x90],
                border: [0xff, 0x3a, 0x3a, 0x3a],
                title: [0xff, 0x90, 0x90, 0x90],
                edge: [0xff, 0x20, 0x20, 0x20],
            },
        }
    }

    pub fn for_state(&self, active: bool) -> &ColorMap {
        if active { &self.active } else { &self.inactive }
    }
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self::dark()
    }
}
