//! Protocol implementations grouped by **upstream stability class**.
//!
//! Mirrors how [wayland-protocols](https://gitlab.freedesktop.org/wayland/wayland-protocols)
//! and Smithay organize XML (core / stable / staging / unstable / ext) plus
//! **community** trees (`wlr`, plasma, …). Native code lives under the matching
//! folder so dependencies and capability flags stay honest.
//!
//! ```text
//! protocols.rs          // matrix + ProtocolClass / ProtocolSpec
//! protocols/
//!   core.rs + core/     // wl_* from wayland.xml (always required)
//!   stable.rs           // wayland-protocols stable/ (xdg-shell, …)
//!   staging.rs          // wayland-protocols staging/
//!   unstable.rs         // legacy zwp_* only if still needed
//!   ext.rs              // ext-* extensions
//!   community.rs + community/
//!     wlr.rs            // wlroots-adjacent (layer-shell, …)
//! ```
//!
//! Wire bindings still come from `wayland-client` / `wayland-protocols*`.
//! These modules own **state machines and Compio-facing behavior**, not XML.

pub mod community;
pub mod core;
pub mod ext;
pub mod stable;
pub mod staging;
pub mod unstable;

/// Upstream protocol stability / origin class.
///
/// Used for capability reporting, feature gating, and deciding how hard a
/// missing global is (core missing = fatal; community missing = optional).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ProtocolClass {
    /// `wayland.xml` core interfaces (`wl_display`, `wl_compositor`, …).
    Core,
    /// `wayland-protocols` **stable/**.
    Stable,
    /// `wayland-protocols` **staging/** (may still change; prefer version checks).
    Staging,
    /// `wayland-protocols` **unstable/** (legacy; avoid new use).
    Unstable,
    /// `ext-*` extensions published under wayland-protocols `ext/`.
    Ext,
    /// Non-FDO community protocols (wlroots, plasma, …).
    Community,
}

impl ProtocolClass {
    /// Whether the class is part of the FDO core + stable baseline.
    pub const fn is_baseline(self) -> bool {
        matches!(self, Self::Core | Self::Stable)
    }

    /// Whether absence of the global should be treated as a soft capability.
    pub const fn is_optional_by_default(self) -> bool {
        matches!(
            self,
            Self::Staging | Self::Unstable | Self::Ext | Self::Community
        )
    }
}

/// One bindable global the native stack cares about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolSpec {
    pub interface: &'static str,
    pub class: ProtocolClass,
    /// Inclusive minimum version we require when the global is present.
    pub min_version: u32,
}

/// Globals this crate understands, classified by upstream stability.
///
/// Order is documentation / discovery only; bind order is decided by
/// dependency (compositor before surface roles, seat before text-input, …).
///
/// Alias: [`FIKA_PROTOCOL_MATRIX`] (historical name kept for compatibility).
pub const PROTOCOL_MATRIX: &[ProtocolSpec] = &[
    // —— core ——
    ProtocolSpec {
        interface: "wl_compositor",
        class: ProtocolClass::Core,
        min_version: 4,
    },
    ProtocolSpec {
        interface: "wl_subcompositor",
        class: ProtocolClass::Core,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "wl_shm",
        class: ProtocolClass::Core,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "wl_seat",
        class: ProtocolClass::Core,
        min_version: 5,
    },
    ProtocolSpec {
        interface: "wl_data_device_manager",
        class: ProtocolClass::Core,
        min_version: 3,
    },
    ProtocolSpec {
        interface: "wl_output",
        class: ProtocolClass::Core,
        min_version: 2,
    },
    // —— stable ——
    ProtocolSpec {
        interface: "xdg_wm_base",
        class: ProtocolClass::Stable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "wp_viewporter",
        class: ProtocolClass::Stable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "wp_presentation",
        class: ProtocolClass::Stable,
        min_version: 1,
    },
    ProtocolSpec {
        // Stable linux-dmabuf-v1; bind versions 3..=5 (Mesa needs ≥3, feedback ≥4).
        interface: "zwp_linux_dmabuf_v1",
        class: ProtocolClass::Stable,
        min_version: 3,
    },
    // —— unstable (still widely deployed) ——
    ProtocolSpec {
        interface: "zwp_primary_selection_device_manager_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zwp_idle_inhibit_manager_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zxdg_exporter_v2",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zxdg_importer_v2",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    // —— staging ——
    ProtocolSpec {
        interface: "wp_fractional_scale_manager_v1",
        class: ProtocolClass::Staging,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "wp_cursor_shape_manager_v1",
        class: ProtocolClass::Staging,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "xdg_activation_v1",
        class: ProtocolClass::Staging,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "xdg_wm_dialog_v1",
        class: ProtocolClass::Staging,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "xdg_toplevel_icon_manager_v1",
        class: ProtocolClass::Staging,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zwp_text_input_manager_v3",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zwp_pointer_constraints_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zwp_relative_pointer_manager_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zwp_pointer_gestures_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "zxdg_decoration_manager_v1",
        class: ProtocolClass::Unstable,
        min_version: 1,
    },
    // —— ext ——
    ProtocolSpec {
        interface: "ext_background_effect_manager_v1",
        class: ProtocolClass::Ext,
        min_version: 1,
    },
    ProtocolSpec {
        interface: "ext_idle_notifier_v1",
        class: ProtocolClass::Ext,
        min_version: 1,
    },
    // —— community / wlr ——
    ProtocolSpec {
        interface: "zwlr_layer_shell_v1",
        class: ProtocolClass::Community,
        min_version: 1,
    },
];

/// Historical alias for [`PROTOCOL_MATRIX`] (Fika workspace name).
pub const FIKA_PROTOCOL_MATRIX: &[ProtocolSpec] = PROTOCOL_MATRIX;

/// Filter the matrix by class (for phased implementation).
pub fn specs_in_class(class: ProtocolClass) -> impl Iterator<Item = &'static ProtocolSpec> {
    PROTOCOL_MATRIX
        .iter()
        .filter(move |spec| spec.class == class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_covers_core_and_xdg() {
        assert!(
            PROTOCOL_MATRIX
                .iter()
                .any(|s| s.interface == "wl_compositor" && s.class == ProtocolClass::Core)
        );
        assert!(
            PROTOCOL_MATRIX
                .iter()
                .any(|s| s.interface == "xdg_wm_base" && s.class == ProtocolClass::Stable)
        );
        assert!(PROTOCOL_MATRIX.iter().any(|s| {
            s.interface == "wp_fractional_scale_manager_v1" && s.class == ProtocolClass::Staging
        }));
        assert!(
            PROTOCOL_MATRIX.iter().any(
                |s| s.interface == "zwlr_layer_shell_v1" && s.class == ProtocolClass::Community
            )
        );
        assert!(
            PROTOCOL_MATRIX
                .iter()
                .any(|s| s.interface == "ext_idle_notifier_v1" && s.class == ProtocolClass::Ext)
        );
        assert!(
            PROTOCOL_MATRIX
                .iter()
                .any(|s| s.interface == "zxdg_exporter_v2" && s.class == ProtocolClass::Unstable)
        );
        // Alias stays in lockstep.
        assert_eq!(PROTOCOL_MATRIX.len(), FIKA_PROTOCOL_MATRIX.len());
    }

    #[test]
    fn optional_classes_match_policy() {
        assert!(!ProtocolClass::Core.is_optional_by_default());
        assert!(!ProtocolClass::Stable.is_optional_by_default());
        assert!(ProtocolClass::Staging.is_optional_by_default());
        assert!(ProtocolClass::Community.is_optional_by_default());
    }

    #[test]
    fn specs_in_class_filters() {
        let core: Vec<_> = specs_in_class(ProtocolClass::Core)
            .map(|s| s.interface)
            .collect();
        assert!(core.contains(&"wl_seat"));
        assert!(!core.contains(&"xdg_wm_base"));
    }
}
