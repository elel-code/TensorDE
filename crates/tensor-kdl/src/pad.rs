//! Owned input padding for SWAR/SIMD (Glaze `padding_bytes`).
//!
//! Cite:
//! - `references/glaze/include/glaze/core/opts.hpp` — `padding_bytes = 16`
//! - `references/glaze/docs/optimizing-performance.md` — pad `std::string` for
//!   efficient SIMD/SWAR, then treat logical length as original
//!
//! Rust `&str` is immutable and unpadded. Callers that own a `String` (or can
//! allocate once) use [`PaddedInput`] so quote/ws scanners may read past the
//! logical end without per-chunk remainder branches on the final block.
//!
//! **Safety contract:** padding bytes are zeros; KDL never treats trailing NULs
//! as content because parse stops at logical [`PaddedInput::len`]. Scanners that
//! load u64 past `len` but within `len + PAD` see zeros (no false quote/escape).

/// Glaze `padding_bytes` — excess capacity after logical content.
pub const PADDING_BYTES: usize = 16;

/// Owned UTF-8 input with [`PADDING_BYTES`] zero bytes after the logical end.
///
/// Construct with [`Self::new`] / [`Self::from_string`]. Pass
/// [`Self::as_str`] (logical slice only) to parse APIs; the underlying allocation
/// remains padded for internal scanners that receive the full byte buffer via
/// [`Self::padded_bytes`] when implementing zero-copy hot paths.
#[derive(Debug, Clone)]
pub struct PaddedInput {
    /// `content` + 16 zero bytes (not valid as str beyond content_len).
    buf: Vec<u8>,
    content_len: usize,
}

impl PaddedInput {
    /// Copy `input` into a padded buffer (Glaze resize + pad pattern).
    pub fn new(input: &str) -> Self {
        let content_len = input.len();
        let mut buf = Vec::with_capacity(content_len + PADDING_BYTES);
        buf.extend_from_slice(input.as_bytes());
        buf.resize(content_len + PADDING_BYTES, 0);
        Self { buf, content_len }
    }

    /// Consume an owned `String`, reusing its allocation when capacity allows.
    pub fn from_string(mut s: String) -> Self {
        let content_len = s.len();
        s.reserve(PADDING_BYTES);
        let mut buf = std::mem::take(&mut s).into_bytes();
        buf.resize(content_len + PADDING_BYTES, 0);
        Self { buf, content_len }
    }

    /// Replace the logical input while retaining this allocation when it is large enough.
    ///
    /// This is the reusable-buffer half of Glaze's mutable `std::string` read
    /// workflow (`docs/optimizing-performance.md`). The padded tail is restored
    /// on every update, so callers can keep one allocation across configuration
    /// reloads without exposing NUL padding as KDL content.
    pub fn replace(&mut self, input: &str) {
        let content_len = input.len();
        let padded_len = content_len
            .checked_add(PADDING_BYTES)
            .expect("padded KDL input length overflow");
        self.buf.clear();
        // `Vec::reserve` is relative to its *length*, which is zero after the
        // clear. Reserve the complete target length when the retained capacity
        // is too small; reserving only the capacity delta could leave a short
        // buffer and force a second growth during `resize` below.
        if self.buf.capacity() < padded_len {
            self.buf.reserve(padded_len);
        }
        self.buf.extend_from_slice(input.as_bytes());
        self.buf.resize(padded_len, 0);
        self.content_len = content_len;
    }

    /// Logical KDL text (no padding). Safe for all public parse APIs.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: content is original UTF-8; padding is outside this slice.
        // We only expose the content region as str.
        std::str::from_utf8(&self.buf[..self.content_len]).expect("PaddedInput content is UTF-8")
    }

    /// Logical length in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.content_len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.content_len == 0
    }

    /// Full buffer including trailing zeros (for SWAR that may over-read).
    #[inline]
    pub fn padded_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Logical content bytes only.
    #[inline]
    pub fn content_bytes(&self) -> &[u8] {
        &self.buf[..self.content_len]
    }
}

impl AsRef<str> for PaddedInput {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Pad `s` in place with [`PADDING_BYTES`] zeros (Glaze in-place pad).
///
/// After padding, only `original_len` bytes are logical content; use
/// [`unpad_string`] after parsing if the string must shrink back.
pub fn pad_string(s: &mut String) -> usize {
    let original = s.len();
    s.reserve(PADDING_BYTES);
    // SAFETY: extend with NULs — not part of the logical str view if we track len.
    // We keep them in the String so as_bytes() over-reads are defined; as_str()
    // includes NULs which are valid UTF-8. Callers should prefer [`PaddedInput`].
    for _ in 0..PADDING_BYTES {
        s.push('\0');
    }
    original
}

/// Truncate a string previously extended by [`pad_string`] back to `original_len`.
pub fn unpad_string(s: &mut String, original_len: usize) {
    s.truncate(original_len);
}

/// Load up to 8 bytes little-endian from `index` (zeros past `input.len()`).
///
/// When `input` is a [`PaddedInput::padded_bytes`] buffer, callers may pass
/// `index` near the logical end and still get a full u64 of content+padding
/// zeros (Glaze padded over-read).
#[inline(always)]
pub fn load_u64_for_scan(input: &[u8], index: usize) -> u64 {
    let mut buf = [0u8; 8];
    if index >= input.len() {
        return 0;
    }
    let n = (input.len() - index).min(8);
    buf[..n].copy_from_slice(&input[index..index + n]);
    u64::from_le_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_preserves_content() {
        let p = PaddedInput::new("hello");
        assert_eq!(p.as_str(), "hello");
        assert_eq!(p.len(), 5);
        assert_eq!(p.padded_bytes().len(), 5 + PADDING_BYTES);
        assert!(p.padded_bytes()[5..].iter().all(|&b| b == 0));
    }

    #[test]
    fn from_string_reuses() {
        let p = PaddedInput::from_string(String::from("kdl"));
        assert_eq!(p.as_str(), "kdl");
        assert_eq!(p.padded_bytes().len(), 3 + PADDING_BYTES);
    }

    #[test]
    fn replace_reuses_capacity_and_restores_padding() {
        let mut p = PaddedInput::new("a sufficiently long first input");
        let capacity = p.buf.capacity();

        p.replace("short");

        assert_eq!(p.as_str(), "short");
        assert_eq!(p.len(), 5);
        assert_eq!(p.buf.capacity(), capacity);
        assert!(p.padded_bytes()[p.len()..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn replace_grows_to_include_padding_before_the_next_read() {
        let mut p = PaddedInput::new("x");
        p.replace("a longer replacement input");

        assert!(p.buf.capacity() >= p.len() + PADDING_BYTES);
        assert_eq!(p.padded_bytes().len(), p.len() + PADDING_BYTES);
        assert!(p.padded_bytes()[p.len()..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn pad_unpad_string() {
        let mut s = String::from("abc");
        let n = pad_string(&mut s);
        assert_eq!(n, 3);
        assert_eq!(s.len(), 3 + PADDING_BYTES);
        unpad_string(&mut s, n);
        assert_eq!(s, "abc");
    }

    #[test]
    fn load_u64_reads_padding_zeros() {
        let p = PaddedInput::new("hi\"");
        // Near end of content: load may include padding zeros after `"`.
        let v = load_u64_for_scan(p.padded_bytes(), 2);
        assert_eq!(v as u8, b'"');
        // Past content into pure padding.
        assert_eq!(load_u64_for_scan(p.padded_bytes(), p.len()), 0);
    }
}
