//! Wayland protocol tiers (wayland-protocols aligned).
//!
//! Implementation priority is **higher tier first** for the same capability.
//! Community (`wlr`) is a documented stopgap, not the default design target.
//! Wire adapters consume this catalog; it has no dependency on Smithay or a
//! live Wayland display.

/// Protocol maturity / origin tier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ProtocolTier {
    /// `wayland.xml` core (`wl_*`).
    Core = 0,
    /// wayland-protocols stable + mature desktop shell (`xdg_*`, stable `wp_*`).
    Stable = 1,
    /// Staging and `ext-*` (first-class for new desktop features).
    StagingExt = 2,
    /// Legacy unstable (`z*` interfaces still common in the wild).
    Unstable = 3,
    /// Community extensions (`zwlr_*`, plasma, misc).
    Community = 4,
    /// Compositor-private / proprietary (out of scope by default).
    Proprietary = 5,
}

impl ProtocolTier {
    /// Short label for logs and capability summaries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Stable => "stable",
            Self::StagingExt => "staging-ext",
            Self::Unstable => "unstable",
            Self::Community => "community",
            Self::Proprietary => "proprietary",
        }
    }

    /// Whether Tensor should invest in new work at this tier by default.
    #[must_use]
    pub const fn preferred_for_new_work(self) -> bool {
        matches!(self, Self::Core | Self::Stable | Self::StagingExt)
    }
}

/// Named advertised capability and its protocol tier.
///
/// Flat `ProtocolCapabilities` stays boolean for IPC/tests; this table is the
/// human/architecture index used by docs alignment tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolCapabilityRef {
    pub name: &'static str,
    pub tier: ProtocolTier,
    /// When set, a higher-tier peer should be preferred for the same job.
    pub prefer_over_community: bool,
}

/// Static catalog of Tensor-advertised (or intentionally community) surfaces.
///
/// Keep in sync with `ProtocolGlobals` / `docs/protocol-surface.md`. Not every
/// core `wl_*` is listed—only long-lived globals beyond the shell baseline.
pub const PROTOCOL_CATALOG: &[ProtocolCapabilityRef] = &[
    // Tier 1 — stable standard
    ProtocolCapabilityRef {
        name: "xdg-shell",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "xdg-decoration",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "xdg-activation",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "viewporter",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "presentation-time",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "linux-dmabuf",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "primary-selection",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "tablet-v2",
        tier: ProtocolTier::Stable,
        prefer_over_community: false,
    },
    // Tier 2 — staging / ext
    ProtocolCapabilityRef {
        name: "fractional-scale",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "xdg-dialog",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "xdg-toplevel-drag",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "ext-session-lock",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "ext-foreign-toplevel-list",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: true,
    },
    ProtocolCapabilityRef {
        name: "ext-data-control",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: true,
    },
    ProtocolCapabilityRef {
        name: "ext-background-effect",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "ext-image-capture-source",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: true,
    },
    ProtocolCapabilityRef {
        name: "ext-image-copy-capture",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: true,
    },
    ProtocolCapabilityRef {
        name: "ext-workspace",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "wp-security-context",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "cursor-shape",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "content-type",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "alpha-modifier",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "fifo",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "commit-timing",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "tearing-control",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "ext-transient-seat",
        tier: ProtocolTier::StagingExt,
        prefer_over_community: false,
    },
    // Tier 3 — unstable (still common)
    ProtocolCapabilityRef {
        name: "pointer-gestures",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "pointer-constraints",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "relative-pointer",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "idle-inhibit",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "text-input-v3",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "input-method-v2",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "virtual-keyboard-v1",
        tier: ProtocolTier::Unstable,
        prefer_over_community: false,
    },
    // Tier 4 — community (documented exceptions)
    ProtocolCapabilityRef {
        name: "wlr-layer-shell",
        tier: ProtocolTier::Community,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "wlr-output-management",
        tier: ProtocolTier::Community,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "wlr-data-control",
        tier: ProtocolTier::Community,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "zwlr-virtual-pointer",
        tier: ProtocolTier::Community,
        prefer_over_community: false,
    },
    ProtocolCapabilityRef {
        name: "zwlr-gamma-control",
        tier: ProtocolTier::Community,
        prefer_over_community: false,
    },
];

/// Look up a catalog entry by stable name.
#[cfg_attr(not(test), allow(dead_code))]
pub fn catalog_entry(name: &str) -> Option<&'static ProtocolCapabilityRef> {
    PROTOCOL_CATALOG.iter().find(|entry| entry.name == name)
}

/// Count catalog entries at or above the given tier (for tests / IPC diagnostics).
#[cfg_attr(not(test), allow(dead_code))]
pub fn catalog_count_at_most(tier: ProtocolTier) -> usize {
    PROTOCOL_CATALOG
        .iter()
        .filter(|entry| entry.tier <= tier)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_ext_is_preferred_for_new_work() {
        assert!(ProtocolTier::StagingExt.preferred_for_new_work());
        assert!(!ProtocolTier::Community.preferred_for_new_work());
        assert!(!ProtocolTier::Proprietary.preferred_for_new_work());
        assert_eq!(ProtocolTier::StagingExt.as_str(), "staging-ext");
        assert_eq!(ProtocolTier::Community.as_str(), "community");
    }

    #[test]
    fn ext_capture_outranks_community_screencopy_policy() {
        let capture = catalog_entry("ext-image-copy-capture").unwrap();
        assert_eq!(capture.tier, ProtocolTier::StagingExt);
        assert!(capture.prefer_over_community);
        let layer = catalog_entry("wlr-layer-shell").unwrap();
        assert_eq!(layer.tier, ProtocolTier::Community);
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut names: Vec<_> = PROTOCOL_CATALOG.iter().map(|e| e.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn tier_order_matches_wayland_protocols_priority() {
        assert!(ProtocolTier::Core < ProtocolTier::Stable);
        assert!(ProtocolTier::Stable < ProtocolTier::StagingExt);
        assert!(ProtocolTier::StagingExt < ProtocolTier::Unstable);
        assert!(ProtocolTier::Unstable < ProtocolTier::Community);
    }

    #[test]
    fn catalog_count_includes_community_exceptions() {
        let through_staging = catalog_count_at_most(ProtocolTier::StagingExt);
        let through_community = catalog_count_at_most(ProtocolTier::Community);
        assert!(through_community > through_staging);
        assert!(through_staging >= 10);
    }
}
