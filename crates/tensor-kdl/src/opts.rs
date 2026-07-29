//! Compile-time-shaped read options — Glaze `glz::opts`
//! (`references/glaze/include/glaze/core/opts.hpp`).
//!
//! Glaze monomorphizes on `template <auto Opts>` (`core/read.hpp`). Rust cannot
//! use a struct as a const generic parameter (only integers / bool / char), so
//! policy is also exposed as a **packed `u8` bitset** for
//! `fn foo<const OPTS: u8>(...)` call sites (P-G4).
//!
//! Runtime [`Opts`] remains the ergonomic value type; hot paths that need
//! monomorphized branches should take `const OPTS: u8` and use
//! [`flag_error_on_unknown`] / [`Opts::from_bits`].
//!
//! Defaults match Glaze unless KDL syntax forces a divergence (documented).

/// Bit 0 — Glaze `error_on_unknown_keys` (default set).
pub const FLAG_ERROR_ON_UNKNOWN: u8 = 1 << 0;
/// Bit 1 — Glaze `error_on_missing_keys` (default clear).
pub const FLAG_ERROR_ON_MISSING: u8 = 1 << 1;
/// Bit 2 — Glaze `partial_read` (default clear).
pub const FLAG_PARTIAL_READ: u8 = 1 << 2;
/// Bit 3 — trailing validation after document (default set; KDL full parse).
pub const FLAG_VALIDATE_TRAILING: u8 = 1 << 3;

/// Packed default opts (`Opts::new().bits()`).
///
/// `error_on_unknown | validate_trailing` = bits 0 and 3.
pub const OPTS_DEFAULT: u8 = FLAG_ERROR_ON_UNKNOWN | FLAG_VALIDATE_TRAILING;
/// Packed lenient opts (`Opts::lenient().bits()`).
pub const OPTS_LENIENT: u8 = FLAG_VALIDATE_TRAILING;
/// Packed partial-read opts (`Opts::partial().bits()`).
pub const OPTS_PARTIAL: u8 = FLAG_ERROR_ON_UNKNOWN | FLAG_PARTIAL_READ;

/// Read/write policy flags (Glaze `struct opts` user-configurable fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opts {
    /// Glaze `error_on_unknown_keys` (default `true`).
    ///
    /// For KDL typed decode: unknown **properties** / unexpected **children**
    /// when a schema is strict. Document-level free nodes still parse into DOM.
    pub error_on_unknown_keys: bool,

    /// Glaze `error_on_missing_keys` (default `false`).
    ///
    /// When true, required fields absent after a visit-fill finish are errors
    /// (already the derive default for non-`Option` fields). When false, future
    /// work may soft-default missing keys; today required fields still error so
    /// suite/typed configs stay strict.
    pub error_on_missing_keys: bool,

    /// Glaze `partial_read` (default `false`).
    ///
    /// When true, stop after the first top-level node is delivered to a visitor
    /// (Glaze: exit after deepest structural object of interest is filled).
    /// Full-document DOM parse ignores this unless using [`crate::visit_document`].
    pub partial_read: bool,

    /// Glaze `validate_trailing_whitespace`-class check: after a successful
    /// value/document parse, require only line-space until EOF.
    ///
    /// Always on for KDL document parse (spec); kept for API parity.
    pub validate_trailing: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self::new()
    }
}

impl Opts {
    /// Glaze `opts{}` defaults from `opts.hpp`.
    pub const fn new() -> Self {
        Self {
            error_on_unknown_keys: true,
            error_on_missing_keys: false,
            partial_read: false,
            validate_trailing: true,
        }
    }

    /// Lenient config: unknown keys skipped when a skip path exists.
    pub const fn lenient() -> Self {
        Self {
            error_on_unknown_keys: false,
            error_on_missing_keys: false,
            partial_read: false,
            validate_trailing: true,
        }
    }

    /// Partial top-level read (Glaze `partial_read = true`).
    pub const fn partial() -> Self {
        Self {
            error_on_unknown_keys: true,
            error_on_missing_keys: false,
            partial_read: true,
            validate_trailing: false,
        }
    }

    /// Pack into a `u8` for const-generic monomorphization (Glaze `auto Opts`).
    #[inline(always)]
    pub const fn bits(self) -> u8 {
        let mut b = 0u8;
        if self.error_on_unknown_keys {
            b |= FLAG_ERROR_ON_UNKNOWN;
        }
        if self.error_on_missing_keys {
            b |= FLAG_ERROR_ON_MISSING;
        }
        if self.partial_read {
            b |= FLAG_PARTIAL_READ;
        }
        if self.validate_trailing {
            b |= FLAG_VALIDATE_TRAILING;
        }
        b
    }

    /// Unpack from a const-generic bitset.
    #[inline(always)]
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            error_on_unknown_keys: bits & FLAG_ERROR_ON_UNKNOWN != 0,
            error_on_missing_keys: bits & FLAG_ERROR_ON_MISSING != 0,
            partial_read: bits & FLAG_PARTIAL_READ != 0,
            validate_trailing: bits & FLAG_VALIDATE_TRAILING != 0,
        }
    }
}

/// Const-generic friendly accessors (fold at monomorphization sites).
#[inline(always)]
pub const fn flag_error_on_unknown(opts: u8) -> bool {
    opts & FLAG_ERROR_ON_UNKNOWN != 0
}

#[inline(always)]
pub const fn flag_error_on_missing(opts: u8) -> bool {
    opts & FLAG_ERROR_ON_MISSING != 0
}

#[inline(always)]
pub const fn flag_partial_read(opts: u8) -> bool {
    opts & FLAG_PARTIAL_READ != 0
}

#[inline(always)]
pub const fn flag_validate_trailing(opts: u8) -> bool {
    opts & FLAG_VALIDATE_TRAILING != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_roundtrip_presets() {
        assert_eq!(Opts::new().bits(), OPTS_DEFAULT);
        assert_eq!(Opts::lenient().bits(), OPTS_LENIENT);
        assert_eq!(Opts::partial().bits(), OPTS_PARTIAL);
        assert_eq!(Opts::from_bits(OPTS_DEFAULT), Opts::new());
        assert_eq!(Opts::from_bits(OPTS_LENIENT), Opts::lenient());
        assert_eq!(Opts::from_bits(OPTS_PARTIAL), Opts::partial());
    }

    #[test]
    fn const_generic_bits_usable() {
        fn sample<const O: u8>() -> bool {
            flag_error_on_unknown(O)
        }
        assert!(sample::<OPTS_DEFAULT>());
        assert!(!sample::<OPTS_LENIENT>());
    }
}
