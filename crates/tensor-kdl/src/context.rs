//! Reusable parse context — Glaze `glz::context` (`include/glaze/core/context.hpp`).
//!
//! Glaze base fields: `error`, `custom_error_message`, `depth`, `current_file`, `scratch`.
//! Runtime limit *extensions* (optional in Glaze via concepts): `max_string_length`, etc.
//!
//! Policy flags that Glaze keeps in compile-time `glz::opts` also live on
//! [`crate::Opts`]. `Context` still mirrors them for runtime overrides when a
//! caller mutates context between reads (see `docs/kdl/glaze-alignment.md` §3).

use crate::error::{ErrorCode, ErrorCtx};

/// Default maximum children nesting depth (Glaze `max_recursive_depth_limit` = 256).
pub const DEFAULT_MAX_DEPTH: u32 = 256;

/// Runtime parse state + optional limits (Glaze `context` + documented extensions).
#[derive(Debug, Clone)]
pub struct Context {
    /// Glaze `context::error` — last error code from a guarded operation.
    pub error: ErrorCode,
    /// Glaze `context::custom_error_message`.
    pub custom_error_message: Option<CowStr>,
    /// Glaze `context::depth`.
    pub depth: u32,
    /// Glaze `context::current_file` (optional path for diagnostics).
    pub current_file: String,
    /// Glaze `context::scratch` — reusable unescape / temp buffer.
    pub scratch: String,
    /// Runtime extension (Glaze `has_runtime_max_string_length`).
    pub max_string_len: usize,
    /// Runtime extension analogue for children / array-like caps.
    pub max_children: usize,
    pub max_depth: u32,
    /// Runtime override of [`crate::Opts::error_on_unknown_keys`] (Glaze default true).
    pub error_on_unknown_keys: bool,
    /// Runtime override of [`crate::Opts::error_on_missing_keys`] (Glaze default false).
    pub error_on_missing_keys: bool,
}

/// Small owned/static string for context messages without pulling full Cow into every path.
pub type CowStr = std::borrow::Cow<'static, str>;

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new() -> Self {
        Self {
            error: ErrorCode::None,
            custom_error_message: None,
            depth: 0,
            current_file: String::new(),
            scratch: String::with_capacity(64),
            max_string_len: usize::MAX,
            max_children: usize::MAX,
            max_depth: DEFAULT_MAX_DEPTH,
            error_on_unknown_keys: true,
            error_on_missing_keys: false,
        }
    }

    /// Seed policy bits from [`crate::Opts`] (Glaze: opts are separate from context;
    /// we copy so a single `Context` can still drive existing call sites).
    pub fn with_opts(mut self, opts: crate::Opts) -> Self {
        self.apply_opts(opts);
        self
    }

    pub fn apply_opts(&mut self, opts: crate::Opts) {
        self.error_on_unknown_keys = opts.error_on_unknown_keys;
        self.error_on_missing_keys = opts.error_on_missing_keys;
    }

    /// Clear error state between independent reads that reuse this context
    /// (Glaze overwrites `ctx.error` each call; caller should still check after each).
    pub fn clear_error(&mut self) {
        self.error = ErrorCode::None;
        self.custom_error_message = None;
    }

    pub fn reset_depth(&mut self) {
        self.depth = 0;
    }

    pub fn clear_scratch(&mut self) {
        self.scratch.clear();
    }

    /// Enter one nesting level. On overflow sets `self.error` like Glaze `depth_guard`
    /// (does not panic); returns `false` if the enter failed.
    pub fn try_enter_depth(&mut self) -> bool {
        if self.depth >= self.max_depth {
            self.error = ErrorCode::ExceededMaxDepth;
            self.custom_error_message = Some(std::borrow::Cow::Owned(format!(
                "max depth is {}",
                self.max_depth
            )));
            return false;
        }
        self.depth += 1;
        true
    }

    /// Fallible enter used by the KDL parser (maps guard failure to `ErrorCtx`).
    pub fn enter_depth(&mut self, offset: usize) -> Result<(), ErrorCtx> {
        if !self.try_enter_depth() {
            return Err(
                ErrorCtx::new(ErrorCode::ExceededMaxDepth, offset).with_message(
                    self.custom_error_message
                        .clone()
                        .unwrap_or(std::borrow::Cow::Borrowed("exceeded max depth")),
                ),
            );
        }
        Ok(())
    }

    pub fn leave_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

/// RAII depth guard — Glaze `glz::depth_guard` (`context.hpp`).
///
/// Construction failure is visible via [`Self::entered`] / `bool`, matching Glaze
/// `explicit operator bool() const`.
pub struct DepthGuard<'a> {
    ctx: &'a mut Context,
    entered: bool,
}

impl<'a> DepthGuard<'a> {
    /// Glaze-style: bump depth or set `ctx.error` and leave `entered == false`.
    pub fn new(ctx: &'a mut Context) -> Self {
        let entered = ctx.try_enter_depth();
        Self { ctx, entered }
    }

    /// Parser helper: map failed enter to `ErrorCtx` at `offset`.
    pub fn enter(ctx: &'a mut Context, offset: usize) -> Result<Self, ErrorCtx> {
        let g = Self::new(ctx);
        if !g.entered {
            return Err(ErrorCtx::new(ErrorCode::ExceededMaxDepth, offset)
                .with_message(format!("max depth is {}", g.ctx.max_depth)));
        }
        Ok(g)
    }

    pub const fn entered(&self) -> bool {
        self.entered
    }
}

impl std::ops::Deref for DepthGuard<'_> {
    type Target = Context;
    fn deref(&self) -> &Self::Target {
        self.ctx
    }
}

impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        if self.entered {
            self.ctx.leave_depth();
        }
    }
}
