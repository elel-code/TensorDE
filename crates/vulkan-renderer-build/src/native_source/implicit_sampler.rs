//! Register assignment for legacy implicit combined-sampler declarations.

use std::collections::BTreeSet;

use super::{SampledImage, declaration_parts, is_sampled_image_type};

pub(super) fn parse(line: &str) -> Result<Option<SampledImage>, String> {
    let Some(declaration) = line.strip_prefix("uniform ") else {
        return Ok(None);
    };
    let (sampler_type, name) = declaration_parts(declaration)?;
    if !is_sampled_image_type(&sampler_type) {
        return Ok(None);
    }
    if name.contains('[') {
        return Err(format!("sampled-image arrays are not supported: {name}"));
    }
    Ok(Some(SampledImage {
        name,
        sampler_type,
        binding: 0,
        implicit_binding: true,
    }))
}

/// Wallpaper Engine assigns registers in active declaration order, never from
/// a `g_TextureN` spelling. Therefore g_Texture0/g_Texture2 becomes t0/t1.
pub(super) fn reindex(sampled_images: &mut [SampledImage]) {
    let explicit = sampled_images
        .iter()
        .filter(|sampled| !sampled.implicit_binding)
        .map(|sampled| sampled.binding)
        .collect::<BTreeSet<_>>();
    let mut next = 0u32;
    for sampled in sampled_images
        .iter_mut()
        .filter(|sampled| sampled.implicit_binding)
    {
        while explicit.contains(&next) {
            next = next.saturating_add(1);
        }
        sampled.binding = next;
        next = next.saturating_add(1);
    }
}
