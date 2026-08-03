//! Native-Slang catalog stages for Wallpaper Engine's installed Depth Parallax effect.
//!
//! References:
//! - `reverse-engineered/gilder/shaders/effects/depthparallax.{vert,frag}`
//! - `reverse-engineered/gilder/effects/effect-semantics.md`

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct DepthParallaxCatalogVariant {
    pub(super) quality: u8,
    pub(super) mask_enabled: bool,
}

pub(super) fn catalog_variants() -> impl Iterator<Item = DepthParallaxCatalogVariant> {
    (0..=2).flat_map(|quality| {
        [false, true]
            .into_iter()
            .map(move |mask_enabled| DepthParallaxCatalogVariant {
                quality,
                mask_enabled,
            })
    })
}

pub(super) fn catalog_key(variant: DepthParallaxCatalogVariant) -> String {
    format!(
        "effects/depthparallax__GILDER_CATALOG_QUALITY_{}__MASK_{}",
        variant.quality,
        u8::from(variant.mask_enabled),
    )
}

pub(super) fn variant_from_catalog_key(key: &str) -> Option<DepthParallaxCatalogVariant> {
    let mut parts = key.split("__");
    (parts.next()? == "effects/depthparallax").then_some(())?;
    let quality = parts
        .next()?
        .strip_prefix("GILDER_CATALOG_QUALITY_")?
        .parse::<u8>()
        .ok()?;
    let mask_enabled = match parts.next()?.strip_prefix("MASK_")? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    (parts.next().is_none() && quality <= 2).then_some(DepthParallaxCatalogVariant {
        quality,
        mask_enabled,
    })
}

pub(super) fn resolver_source(variants: &[DepthParallaxCatalogVariant]) -> String {
    let keys = variants
        .iter()
        .copied()
        .map(catalog_key)
        .map(|key| format!("    {key:?},\n"))
        .collect::<String>();
    format!(
        r#"
const DEPTH_PARALLAX_CATALOG_KEYS: [&str; {count}] = [
{keys}];

fn depth_parallax_catalog_key_for_semantic_key(key: &str) -> Option<&'static str> {{
    let suffix = key.strip_prefix("effects/depthparallax__")?;
    let mut slots = None;
    let mut quality = 1u8;
    let mut mask = None;
    let mut seen_slots = false;
    let mut seen_quality = false;
    let mut seen_mask = false;

    for part in suffix.split("__") {{
        if let Some(value) = part.strip_prefix("SLOTS_") {{
            if seen_slots {{
                return None;
            }}
            slots = u32::from_str_radix(value, 16).ok();
            seen_slots = true;
        }} else if let Some(value) = part.strip_prefix("QUALITY_") {{
            if seen_quality {{
                return None;
            }}
            quality = value.parse().ok()?;
            seen_quality = true;
        }} else if let Some(value) = part.strip_prefix("MASK_") {{
            if seen_mask {{
                return None;
            }}
            mask = match value {{
                "0" => Some(false),
                "1" => Some(true),
                _ => return None,
            }};
            seen_mask = true;
        }} else {{
            return None;
        }}
    }}

    if quality > 2 {{
        return None;
    }}
    let slots = slots?;
    let mask = mask.unwrap_or(slots & 0x4 != 0);
    if slots != 0x3 | if mask {{ 0x4 }} else {{ 0 }} {{
        return None;
    }}
    DEPTH_PARALLAX_CATALOG_KEYS
        .get(usize::from(quality) * 2 + usize::from(u8::from(mask)))
        .copied()
}}

"#,
        count = variants.len(),
    )
}

pub(super) fn vertex_source(variant: DepthParallaxCatalogVariant) -> String {
    let mask_output = variant
        .mask_enabled
        .then_some("    [[vk::location(2)]] float2 maskTexCoord : TEXCOORD2;\n");
    let mask_statement = variant.mask_enabled.then_some(
        "    output.maskTexCoord = uv * material.texture2Resolution.zw / material.texture2Resolution.xy;\n",
    );
    format!(
        r#"struct DepthParallaxDrawData
{{
    float4 effectTextureProjectionInverse[4];
}};

cbuffer DepthParallaxDraw : register(b2)
{{
    DepthParallaxDrawData draw;
}}

struct DepthParallaxMaterialData
{{
    float4 texture1Resolution;
    float4 texture2Resolution;
    float2 parallaxPosition;
    float2 scale;
    float sensitivity;
    float center;
    float2 padding;
}};

cbuffer DepthParallaxMaterial : register(b3)
{{
    DepthParallaxMaterialData material;
}}

struct DepthParallaxVertexInput
{{
    uint vertexId : SV_VertexID;
}};

struct DepthParallaxVertexOutput
{{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float2 parallaxOffset : TEXCOORD1;
{mask_output}}};

[[shader("vertex")]]
DepthParallaxVertexOutput main(DepthParallaxVertexInput input)
{{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    float2 uv = position * 0.5 + 0.5;
    float2 projectedDirX = normalize(float2(
        draw.effectTextureProjectionInverse[0].x,
        draw.effectTextureProjectionInverse[1].x));
    float2 projectedDirY = normalize(float2(
        draw.effectTextureProjectionInverse[0].y,
        draw.effectTextureProjectionInverse[1].y));
    float2 parallaxInput = material.parallaxPosition * 2.0 - 1.0;

    DepthParallaxVertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord.xy = uv;
    output.texCoord.zw = uv * material.texture1Resolution.zw / material.texture1Resolution.xy;
    output.parallaxOffset =
        (projectedDirX * parallaxInput.x + projectedDirY * parallaxInput.y) * 0.5 + 0.5;
{mask_statement}    return output;
}}
"#,
        mask_output = mask_output.unwrap_or_default(),
        mask_statement = mask_statement.unwrap_or_default(),
    )
}

pub(super) fn fragment_source(variant: DepthParallaxCatalogVariant) -> String {
    let mask_input = variant
        .mask_enabled
        .then_some("    [[vk::location(2)]] float2 maskTexCoord : TEXCOORD2;\n");
    let mask_resources = variant.mask_enabled.then_some(
        "Texture2D<float4> maskTexture : register(t2);\nSamplerState maskSampler : register(s2);\n",
    );
    let mask_sample = variant
        .mask_enabled
        .then_some("    mask = maskTexture.Sample(maskSampler, input.maskTexCoord).r;\n");
    let albedo = match variant.quality {
        0 => r#"    float2 pointer = float2(input.texCoord.z, 1.0 - input.texCoord.w);
    pointer = (pointer - input.parallaxOffset) * float2(2.0, -2.0) * material.scale * -0.04;
    float2 offset = (depth * 2.0 - 1.0) * pointer * mask;
    float4 albedo = sourceTexture.Sample(sourceSampler, input.texCoord.xy + offset);"#
            .to_owned(),
        quality => {
            let layers = if quality == 1 { 24 } else { 64 };
            format!(
                r#"    float controlSign = step(0.0, material.sensitivity);
    float negativePerspective = -material.sensitivity;
    float perspectiveControl = saturate(material.sensitivity)
        + step(0.0001, negativePerspective);
    float2 parallax = lerp(
        input.parallaxOffset,
        1.0 - input.parallaxOffset,
        controlSign);
    float2 coords = lerp(
        input.texCoord.xy,
        (input.texCoord.xy - 0.5) / (1.0 + material.sensitivity * 0.2) + 0.5,
        controlSign);
    coords -= (parallax * 2.0 - 1.0) * material.center
        * float2(-0.05, 0.05) * material.scale
        * lerp(-1.0, negativePerspective, perspectiveControl);
    float2 pointer = float2(1.0 - input.texCoord.z, input.texCoord.w);
    float2 controlDirection = pointer - parallax;
    controlDirection = lerp(
        float2(1.0 - parallax.x, parallax.y) - 0.5,
        controlDirection * float2(-negativePerspective, negativePerspective),
        perspectiveControl);

    const int layerCount = {layers};
    const float layerDepth = 1.0 / float(layerCount);
    float2 deltaTexCoords = controlDirection * mask * material.scale * 0.1
        / float(layerCount);
    float2 currentTexCoords = coords;
    float currentLayerDepth = 1.0;
    float currentDepth = depthTexture.Sample(depthSampler, currentTexCoords).r;
    for (int layer = 0; currentLayerDepth > currentDepth && layer < layerCount; ++layer)
    {{
        currentTexCoords -= deltaTexCoords;
        currentDepth = depthTexture.Sample(depthSampler, currentTexCoords).r;
        currentLayerDepth -= layerDepth;
    }}
    float2 previousTexCoords = currentTexCoords + deltaTexCoords;
    float afterDepth = currentDepth - currentLayerDepth;
    float beforeDepth = depthTexture.Sample(depthSampler, previousTexCoords).r
        - currentLayerDepth - layerDepth;
    float weight = afterDepth / (afterDepth - beforeDepth);
    float2 finalTexCoords =
        previousTexCoords * weight + currentTexCoords * (1.0 - weight);
    float4 albedo = sourceTexture.Sample(sourceSampler, finalTexCoords);"#
            )
        }
    };
    format!(
        r#"struct DepthParallaxMaterialData
{{
    float4 texture1Resolution;
    float4 texture2Resolution;
    float2 parallaxPosition;
    float2 scale;
    float sensitivity;
    float center;
    float2 padding;
}};

cbuffer DepthParallaxMaterial : register(b3)
{{
    DepthParallaxMaterialData material;
}}

Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> depthTexture : register(t1);
SamplerState depthSampler : register(s1);
{mask_resources}
struct DepthParallaxFragmentInput
{{
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float2 parallaxOffset : TEXCOORD1;
{mask_input}}};

[[shader("fragment")]]
float4 main(DepthParallaxFragmentInput input) : SV_Target0
{{
    float depth = depthTexture.Sample(depthSampler, input.texCoord.zw).r;
    float mask = 1.0;
{mask_sample}{albedo}
    return albedo;
}}
"#,
        mask_resources = mask_resources.unwrap_or_default(),
        mask_input = mask_input.unwrap_or_default(),
        mask_sample = mask_sample.unwrap_or_default(),
    )
}
