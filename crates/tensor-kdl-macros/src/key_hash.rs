//! Glaze `front_hash` and `full_flat` key tables (P-G8a).
//!
//! Cite: `references/glaze/include/glaze/core/reflect.hpp`
//! - `front_bytes_hash_info` — unique front 2/4/8 bytes + `bitmix` seed + bucket table
//! - `full_flat` — full-key hash into `bit_ceil(N²)/2` buckets
//! - `bucket_size(front_hash|full_flat, N)` — `(N==1)?1:bit_ceil(N*N)/2`
//! - `primes_64` seed candidates (`util/primes_64.hpp`)
//! - `bitmix` — `h *= seed; h ^= rotr(h, 49)`

use quote::quote;

/// Subset of Glaze `primes_64` for seed search (first 32 is enough for small N).
pub(crate) const PRIMES_64: &[u64] = &[
    12835920395396008793,
    15149911783463666029,
    15211026597907833541,
    14523965596842631817,
    16449355892475772073,
    15002762636229733759,
    12275448295353509891,
    16826285440568349437,
    17433093378066653197,
    10902769355249605843,
    13434269760430048511,
    11322871945166463571,
    9764742595129026499,
    13799666429485716229,
    14861204462552525359,
    17599486090324515493,
    10266842847898195667,
    13468209895759219897,
    16289274021814922521,
    17204791465022878523,
    17650915497556268801,
    9455725851336774341,
    9961868820920778071,
    18289017266131008167,
    16309921878298474091,
    11652007405601517343,
    17496906368504743207,
    13339901080756288547,
    10018112158103183191,
    14981853847663275059,
    15024425770511821387,
    10063189458099824779,
];

/// Glaze `bitmix`.
#[inline(always)]
pub(crate) const fn bitmix(h: u64, seed: u64) -> u64 {
    let h = h.wrapping_mul(seed);
    h ^ h.rotate_right(49)
}

/// Glaze `bucket_size` for `front_hash` / `full_flat`.
pub(crate) fn glaze_bucket_size(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let prod = n.saturating_mul(n);
    let ceil = prod.next_power_of_two();
    (ceil / 2).max(n.next_power_of_two())
}

fn front_u16(key: &str) -> Option<u64> {
    let b = key.as_bytes();
    if b.len() < 2 {
        return None;
    }
    Some(u64::from(b[0]) | (u64::from(b[1]) << 8))
}

fn front_u32(key: &str) -> Option<u64> {
    let b = key.as_bytes();
    if b.len() < 4 {
        return None;
    }
    Some(
        u64::from(b[0])
            | (u64::from(b[1]) << 8)
            | (u64::from(b[2]) << 16)
            | (u64::from(b[3]) << 24),
    )
}

fn front_u64(key: &str) -> Option<u64> {
    let b = key.as_bytes();
    if b.len() < 8 {
        return None;
    }
    Some(
        u64::from(b[0])
            | (u64::from(b[1]) << 8)
            | (u64::from(b[2]) << 16)
            | (u64::from(b[3]) << 24)
            | (u64::from(b[4]) << 32)
            | (u64::from(b[5]) << 40)
            | (u64::from(b[6]) << 48)
            | (u64::from(b[7]) << 56),
    )
}

/// Front-hash table: seed + slots mapping bucket → key index.
#[derive(Debug, Clone)]
pub(crate) struct FrontHash {
    pub seed: u64,
    pub front_bytes: u8,
    pub table_size: usize,
    pub slots: Vec<usize>,
}

fn try_front_hash(keys: &[&str], front_bytes: u8) -> Option<FrontHash> {
    let n = keys.len();
    if n < 3 {
        return None;
    }
    let min_len = keys.iter().map(|k| k.len()).min()?;
    if min_len < front_bytes as usize {
        return None;
    }
    let chunks: Vec<u64> = keys
        .iter()
        .map(|k| match front_bytes {
            2 => front_u16(k).unwrap(),
            4 => front_u32(k).unwrap(),
            8 => front_u64(k).unwrap(),
            _ => unreachable!(),
        })
        .collect();
    // Uniqueness of front chunks (Glaze front_bytes_hash_info sort+diff check).
    let mut sorted = chunks.clone();
    sorted.sort_unstable();
    for w in sorted.windows(2) {
        if w[0] == w[1] {
            return None;
        }
    }

    let bsize = glaze_bucket_size(n);
    for &seed in PRIMES_64 {
        let mut slots = vec![usize::MAX; bsize];
        let mut ok = true;
        for (i, &chunk) in chunks.iter().enumerate() {
            let h = bitmix(chunk, seed);
            if h == seed {
                ok = false;
                break;
            }
            let bucket = (h as usize) % bsize;
            if slots[bucket] != usize::MAX {
                ok = false;
                break;
            }
            slots[bucket] = i;
        }
        if !ok {
            continue;
        }
        // Seed must not land in an occupied bucket when used as a hash (Glaze).
        let seed_bucket = (seed as usize) % bsize;
        if slots[seed_bucket] != usize::MAX {
            continue;
        }
        return Some(FrontHash {
            seed,
            front_bytes,
            table_size: bsize,
            slots,
        });
    }
    None
}

/// Prefer wider front chunks first (more discrimination), matching Glaze order
/// of trying u16 then u32 then u64 only if shorter fails — Glaze tries u16 first.
pub(crate) fn find_front_hash(keys: &[&str]) -> Option<FrontHash> {
    try_front_hash(keys, 2)
        .or_else(|| try_front_hash(keys, 4))
        .or_else(|| try_front_hash(keys, 8))
}

/// Full-flat table: FNV-1a of whole key mixed with seed (Glaze full_flat role).
#[derive(Debug, Clone)]
pub(crate) struct FullFlatHash {
    pub seed: u64,
    pub table_size: usize,
    pub slots: Vec<usize>,
}

fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(crate) fn find_full_flat(keys: &[&str]) -> Option<FullFlatHash> {
    let n = keys.len();
    if n < 4 {
        return None;
    }
    let hashes: Vec<u64> = keys.iter().map(|k| fnv1a64(k)).collect();
    let bsize = glaze_bucket_size(n);
    for &seed in PRIMES_64 {
        let mut slots = vec![usize::MAX; bsize];
        let mut ok = true;
        for (i, &h0) in hashes.iter().enumerate() {
            let h = bitmix(h0, seed);
            if h == seed {
                ok = false;
                break;
            }
            let bucket = (h as usize) % bsize;
            if slots[bucket] != usize::MAX {
                ok = false;
                break;
            }
            slots[bucket] = i;
        }
        if !ok {
            continue;
        }
        let seed_bucket = (seed as usize) % bsize;
        if slots[seed_bucket] != usize::MAX {
            continue;
        }
        return Some(FullFlatHash {
            seed,
            table_size: bsize,
            slots,
        });
    }
    None
}

struct SlotEmit<'a> {
    key_expr: proc_macro2::TokenStream,
    keys: &'a [String],
    bodies: &'a [proc_macro2::TokenStream],
    seed: u64,
    table_size: usize,
    slots: &'a [usize],
    hash_prelude: proc_macro2::TokenStream,
    fallback: proc_macro2::TokenStream,
}

fn emit_slot_table_match(args: SlotEmit<'_>) -> proc_macro2::TokenStream {
    let SlotEmit {
        key_expr,
        keys,
        bodies,
        seed,
        table_size,
        slots,
        hash_prelude,
        fallback,
    } = args;
    let slot_lits: Vec<_> = slots
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
            #hash_prelude
            let __mixed = {
                let __x = __h.wrapping_mul(#seed);
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

pub(crate) fn emit_front_hash_match(
    key_expr: proc_macro2::TokenStream,
    keys: &[String],
    bodies: &[proc_macro2::TokenStream],
    hash: &FrontHash,
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let n = hash.front_bytes as usize;
    let prelude = quote! {
        let __bytes = __kdl_key.as_bytes();
        if __bytes.len() < #n {
            // too short for front hash — unknown
            let __h: u64 = 0;
            let _ = __h;
            // fall through with zero hash so slot miss
        }
        let __h: u64 = if __bytes.len() < #n {
            0
        } else {
            let mut __acc: u64 = 0;
            let mut __i = 0usize;
            while __i < #n {
                __acc |= (__bytes[__i] as u64) << (8 * __i);
                __i += 1;
            }
            __acc
        };
    };
    emit_slot_table_match(SlotEmit {
        key_expr,
        keys,
        bodies,
        seed: hash.seed,
        table_size: hash.table_size,
        slots: &hash.slots,
        hash_prelude: prelude,
        fallback,
    })
}

pub(crate) fn emit_full_flat_match(
    key_expr: proc_macro2::TokenStream,
    keys: &[String],
    bodies: &[proc_macro2::TokenStream],
    hash: &FullFlatHash,
    fallback: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let prelude = quote! {
        let mut __h: u64 = 0xcbf2_9ce4_8422_2325;
        for __b in __kdl_key.as_bytes() {
            __h ^= *__b as u64;
            __h = __h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    emit_slot_table_match(SlotEmit {
        key_expr,
        keys,
        bodies,
        seed: hash.seed,
        table_size: hash.table_size,
        slots: &hash.slots,
        hash_prelude: prelude,
        fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_hash_finds_for_long_unique_prefixes() {
        // min_len>=2, unique first two bytes
        let keys = ["alpha", "bravo", "charlie", "delta"];
        let h = find_front_hash(&keys).expect("front hash");
        assert!(h.front_bytes >= 2);
        assert_eq!(
            h.slots.iter().filter(|&&s| s != usize::MAX).count(),
            keys.len()
        );
    }

    #[test]
    fn full_flat_finds_when_front_fails() {
        // Short keys, no unique front of 2 — still full flat may work.
        let keys = ["w", "x", "y", "z"];
        // min_len=1 → front fails; full flat should work
        assert!(find_front_hash(&keys).is_none());
        let h = find_full_flat(&keys).expect("full flat");
        assert_eq!(h.slots.iter().filter(|&&s| s != usize::MAX).count(), 4);
    }

    #[test]
    fn bucket_size_matches_glaze_shape() {
        assert_eq!(glaze_bucket_size(1), 1);
        // N=4 → N*N=16 → bit_ceil 16 / 2 = 8, max with next_pow2(4)=4 → 8
        assert_eq!(glaze_bucket_size(4), 8);
    }
}
