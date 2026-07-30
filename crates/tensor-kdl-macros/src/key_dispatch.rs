//! Compile-time key dispatch — Glaze hash strategies for property/child names.
//!
//! Cite: `references/glaze/include/glaze/core/reflect.hpp`
//! - `find_unique_index` → outer byte switch (**P-G6**)
//! - `find_unique_sized_index` → (byte, length) pair unique (**P-G7**)
//! - modular FNV perfect hash (**P-G7**)
//! - `front_hash` / `full_flat` (**P-G8a**, `key_hash.rs`)
//! - Always full-string verify after table hit (Glaze unknown-key safety)
//!
//! Fallback: plain string `match` (Glaze `decode_linear`).

use quote::quote;
use syn::LitByte;

use crate::key_hash::{
    FrontHash, FullFlatHash, emit_front_hash_match, emit_full_flat_match, find_front_hash,
    find_full_flat,
};

/// First byte index where every key has a distinct value (Glaze `find_unique_index`).
pub(crate) fn find_unique_index(keys: &[&str]) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    let min_len = keys.iter().map(|k| k.len()).min()?;
    if min_len == 0 {
        return None;
    }
    'col: for c in 0..min_len {
        let mut seen = [false; 256];
        for k in keys {
            let b = k.as_bytes()[c] as usize;
            if seen[b] {
                continue 'col;
            }
            seen[b] = true;
        }
        return Some(c);
    }
    None
}

/// First byte index where `(byte, length)` pairs are unique (Glaze
/// `find_unique_sized_index`).
///
/// Distinguishes keys that share a byte at column `c` but differ in length
/// (e.g. `"a"` vs `"ab"` at column 0).
pub(crate) fn find_unique_sized_index(keys: &[&str]) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    let min_len = keys.iter().map(|k| k.len()).min()?;
    if min_len == 0 {
        return None;
    }
    'col: for c in 0..min_len {
        let mut pairs: Vec<(u8, usize)> = keys.iter().map(|k| (k.as_bytes()[c], k.len())).collect();
        pairs.sort_unstable();
        for w in pairs.windows(2) {
            if w[0] == w[1] {
                continue 'col;
            }
        }
        return Some(c);
    }
    None
}

/// One arm of a unique-index dispatch: `(byte_at_index, full_key, body)`.
pub(crate) struct UniqueArm {
    pub byte: u8,
    pub key: String,
    pub body: proc_macro2::TokenStream,
}

/// Emit unique-index dispatch: outer byte at `unique_index`, then full-string check.
pub(crate) fn emit_unique_byte_match(
    key_expr: proc_macro2::TokenStream,
    unique_index: usize,
    arms: &[UniqueArm],
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut match_arms = Vec::with_capacity(arms.len());
    for arm in arms {
        let lit = LitByte::new(arm.byte, proc_macro2::Span::call_site());
        let full = &arm.key;
        let body = &arm.body;
        match_arms.push(quote! {
            ::std::option::Option::Some(#lit) if __kdl_key == #full => {
                #body
            }
        });
    }
    quote! {
        {
            let __kdl_key: &str = #key_expr;
            match __kdl_key.as_bytes().get(#unique_index).copied() {
                #(#match_arms)*
                _ => { #fallback }
            }
        }
    }
}

/// Emit sized unique-index: match on `(len, byte_at_index)` then full string.
pub(crate) fn emit_unique_sized_match(
    key_expr: proc_macro2::TokenStream,
    unique_index: usize,
    arms: &[UniqueArm],
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut match_arms = Vec::with_capacity(arms.len());
    for arm in arms {
        let lit = LitByte::new(arm.byte, proc_macro2::Span::call_site());
        let full = &arm.key;
        let len = arm.key.len();
        let body = &arm.body;
        match_arms.push(quote! {
            (#len, ::std::option::Option::Some(#lit)) if __kdl_key == #full => {
                #body
            }
        });
    }
    quote! {
        {
            let __kdl_key: &str = #key_expr;
            match (__kdl_key.len(), __kdl_key.as_bytes().get(#unique_index).copied()) {
                #(#match_arms)*
                _ => { #fallback }
            }
        }
    }
}

/// FNV-1a 64-bit (compile-time and runtime same algorithm).
pub(crate) fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Mix with seed (lightweight stand-in for Glaze `bitmix` role on full keys).
#[inline(always)]
pub(crate) const fn mix_seed(h: u64, seed: u64) -> u64 {
    let x = h.wrapping_mul(seed | 1);
    x ^ x.rotate_right(49)
}

/// Perfect modular hash table when a seed maps all keys to distinct slots.
///
/// `table[slot] = Some(key_index)` or `None` for empty (unknown-key fast path).
#[derive(Debug, Clone)]
pub(crate) struct ModularHash {
    pub seed: u64,
    pub table_size: usize,
    /// Parallel to `0..table_size`: key index or `usize::MAX` if empty.
    pub slots: Vec<usize>,
}

/// Search for a modular perfect hash (Glaze modular integer-key idea for strings).
///
/// Tries table sizes from `next_pow2(N)` upward and a fixed set of seeds.
pub(crate) fn find_modular_hash(keys: &[&str]) -> Option<ModularHash> {
    let n = keys.len();
    if n < 4 {
        // Prefer string match / unique-index for tiny sets.
        return None;
    }
    let hashes: Vec<u64> = keys.iter().map(|k| fnv1a64(k)).collect();

    // Seeds: odd multipliers (Glaze bitmix multiplies by seed).
    const SEEDS: &[u64] = &[
        0x9e37_79b9_7f4a_7c15,
        0x517c_c1b7_2722_0a95,
        0x6a09_e667_f3bc_c909,
        0x2127_599b_f432_5c37,
        0x8803_55f2_1e6d_1965,
        0xc6a4_a793_5bd1_e995,
        0x85eb_ca77_c2b2_ae63,
        0x27d4_eb2f_1656_67c5,
        1,
        3,
        5,
        7,
        11,
        13,
        17,
        19,
        23,
        29,
        31,
        37,
        41,
        43,
        47,
        53,
    ];

    let mut size = n.next_power_of_two().max(8);
    let max_size = (n * 4).next_power_of_two().max(size);
    while size <= max_size {
        for &seed in SEEDS {
            let mut slots = vec![usize::MAX; size];
            let mut ok = true;
            for (i, &h) in hashes.iter().enumerate() {
                let slot = (mix_seed(h, seed) as usize) % size;
                if slots[slot] != usize::MAX {
                    ok = false;
                    break;
                }
                slots[slot] = i;
            }
            if ok {
                return Some(ModularHash {
                    seed,
                    table_size: size,
                    slots,
                });
            }
        }
        size *= 2;
    }
    None
}

/// Emit modular perfect-hash dispatch with full-string verify.
pub(crate) fn emit_modular_hash_match(
    key_expr: proc_macro2::TokenStream,
    keys: &[String],
    bodies: &[proc_macro2::TokenStream],
    hash: &ModularHash,
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let seed = hash.seed;
    let table_size = hash.table_size;
    // Static table of optional key indices.
    let slot_lits: Vec<_> = hash
        .slots
        .iter()
        .map(|&idx| {
            if idx == usize::MAX {
                quote! { ::std::option::Option::None }
            } else {
                quote! { ::std::option::Option::Some(#idx) }
            }
        })
        .collect();

    let mut case_arms = Vec::new();
    for (i, (k, body)) in keys.iter().zip(bodies.iter()).enumerate() {
        case_arms.push(quote! {
            ::std::option::Option::Some(#i) if __kdl_key == #k => { #body }
        });
    }

    quote! {
        {
            let __kdl_key: &str = #key_expr;
            // FNV-1a + seed mix + modular slot (Glaze modular perfect-hash role).
            let mut __h: u64 = 0xcbf2_9ce4_8422_2325;
            for __b in __kdl_key.as_bytes() {
                __h ^= *__b as u64;
                __h = __h.wrapping_mul(0x0100_0000_01b3);
            }
            let __mixed = {
                let __x = __h.wrapping_mul(#seed | 1);
                __x ^ __x.rotate_right(49)
            };
            static __KDL_SLOTS: [Option<usize>; #table_size] = [
                #(#slot_lits,)*
            ];
            let __slot = (__mixed as usize) % #table_size;
            match __KDL_SLOTS[__slot] {
                #(#case_arms)*
                _ => { #fallback }
            }
        }
    }
}

/// Dispatch strategy chosen at derive time (Glaze `make_keys_info` preference order).
pub(crate) enum KeyStrategy {
    UniqueByte(usize),
    UniqueSized(usize),
    Front(FrontHash),
    Modular(ModularHash),
    FullFlat(FullFlatHash),
    Linear,
}

pub(crate) fn choose_strategy(keys: &[&str]) -> KeyStrategy {
    if keys.len() < 3 {
        return KeyStrategy::Linear;
    }
    // Glaze: unique_index before front_hash before sized before full_flat.
    if let Some(idx) = find_unique_index(keys) {
        return KeyStrategy::UniqueByte(idx);
    }
    if let Some(h) = find_front_hash(keys) {
        return KeyStrategy::Front(h);
    }
    if let Some(idx) = find_unique_sized_index(keys) {
        return KeyStrategy::UniqueSized(idx);
    }
    if let Some(h) = find_modular_hash(keys) {
        return KeyStrategy::Modular(h);
    }
    if let Some(h) = find_full_flat(keys) {
        return KeyStrategy::FullFlat(h);
    }
    KeyStrategy::Linear
}

/// Top-level emit used by visit_fill / named stream.
pub(crate) fn emit_key_strategy_match(
    key_expr: proc_macro2::TokenStream,
    keys: &[String],
    bodies: &[proc_macro2::TokenStream],
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    debug_assert_eq!(keys.len(), bodies.len());
    if keys.is_empty() {
        return fallback;
    }
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    match choose_strategy(&key_refs) {
        KeyStrategy::UniqueByte(idx) => {
            let arms: Vec<UniqueArm> = keys
                .iter()
                .zip(bodies.iter())
                .map(|(k, body)| UniqueArm {
                    byte: k.as_bytes()[idx],
                    key: k.clone(),
                    body: body.clone(),
                })
                .collect();
            emit_unique_byte_match(key_expr, idx, &arms, fallback)
        }
        KeyStrategy::UniqueSized(idx) => {
            let arms: Vec<UniqueArm> = keys
                .iter()
                .zip(bodies.iter())
                .map(|(k, body)| UniqueArm {
                    byte: k.as_bytes()[idx],
                    key: k.clone(),
                    body: body.clone(),
                })
                .collect();
            emit_unique_sized_match(key_expr, idx, &arms, fallback)
        }
        KeyStrategy::Front(h) => emit_front_hash_match(key_expr, keys, bodies, &h, fallback),
        KeyStrategy::Modular(h) => emit_modular_hash_match(key_expr, keys, bodies, &h, fallback),
        KeyStrategy::FullFlat(h) => emit_full_flat_match(key_expr, keys, bodies, &h, fallback),
        KeyStrategy::Linear => {
            let mut str_arms = Vec::new();
            for (k, body) in keys.iter().zip(bodies.iter()) {
                str_arms.push(quote! {
                    #k => { #body }
                });
            }
            quote! {
                match #key_expr {
                    #(#str_arms)*
                    _ => { #fallback }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_first_byte() {
        let keys = ["alpha", "beta", "gamma"];
        assert_eq!(find_unique_index(&keys), Some(0));
    }

    #[test]
    fn unique_later_byte() {
        let keys = ["xxa", "xxb", "xxc"];
        assert_eq!(find_unique_index(&keys), Some(2));
    }

    #[test]
    fn sized_unique_for_prefix_collision() {
        // col0 both 'a' → unique_index fails; sized (a,1) vs (a,2) works.
        let keys = ["a", "ab", "abc"];
        assert_eq!(find_unique_index(&keys), None);
        assert_eq!(find_unique_sized_index(&keys), Some(0));
    }

    #[test]
    fn modular_finds_table_for_similar_keys() {
        // No unique column in min_len for these if they share prefixes heavily.
        let keys = [
            "item_a", "item_b", "item_c", "item_d", "item_e", "item_f", "item_g", "item_h",
        ];
        // First differing might be last char — unique at index 5 ('a'..'h').
        assert!(find_unique_index(&keys).is_some() || find_modular_hash(&keys).is_some());
    }

    #[test]
    fn modular_for_no_unique_column() {
        // Construct keys where every column in min_len has a duplicate.
        // min_len=2: "aa","ab","ba","bb" — col0 has a,a,b,b dups; col1 a,b,a,b dups.
        let keys = ["aa", "ab", "ba", "bb"];
        assert_eq!(find_unique_index(&keys), None);
        // sized: (a,2),(a,2) for aa,ab at col0 — still dups for same length.
        // col0: (a,2),(a,2),(b,2),(b,2) — dups
        // col1: (a,2),(b,2),(a,2),(b,2) — dups
        assert_eq!(find_unique_sized_index(&keys), None);
        let h = find_modular_hash(&keys).expect("modular should solve 4 keys");
        assert_eq!(h.slots.iter().filter(|&&s| s != usize::MAX).count(), 4);
        // All key indices present exactly once.
        let mut present = [false; 4];
        for &s in &h.slots {
            if s != usize::MAX {
                present[s] = true;
            }
        }
        assert!(present.iter().all(|&p| p));
    }

    #[test]
    fn threshold_skips_tiny() {
        let keys = ["a", "b"];
        assert!(matches!(choose_strategy(&keys), KeyStrategy::Linear));
    }

    #[test]
    fn choose_prefers_unique_byte() {
        let keys = ["alpha", "beta", "gamma"];
        assert!(matches!(choose_strategy(&keys), KeyStrategy::UniqueByte(0)));
    }

    #[test]
    fn choose_sized_when_needed() {
        let keys = ["a", "ab", "abc"];
        assert!(matches!(
            choose_strategy(&keys),
            KeyStrategy::UniqueSized(0)
        ));
    }

    #[test]
    fn choose_front_or_modular_for_grid() {
        // min_len=2 unique front u16 → front_hash (Glaze preference before modular).
        let keys = ["aa", "ab", "ba", "bb"];
        assert!(matches!(
            choose_strategy(&keys),
            KeyStrategy::Front(_) | KeyStrategy::Modular(_)
        ));
    }

    #[test]
    fn choose_unique_byte_for_single_char() {
        // Distinct first (only) bytes → UniqueByte, not full_flat.
        let keys = ["w", "x", "y", "z"];
        assert!(matches!(choose_strategy(&keys), KeyStrategy::UniqueByte(0)));
    }
}
