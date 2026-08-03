//! Native-Slang catalog stages for the proven non-audio installed Shake variants.
//!
//! References:
//! - `reverse-engineered/gilder/shaders/effects/shake.{vert,frag}`
//! - `reverse-engineered/gilder/effects/effect-semantics.md`

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ShakeCatalogVariant {
    pub(super) direction: u8,
}

pub(super) fn catalog_variants() -> impl Iterator<Item = ShakeCatalogVariant> {
    (0..=2).map(|direction| ShakeCatalogVariant { direction })
}

pub(super) fn catalog_key(variant: ShakeCatalogVariant) -> String {
    format!(
        "effects/shake__GILDER_CATALOG_DIRECTION_{}",
        variant.direction
    )
}

pub(super) fn variant_from_catalog_key(key: &str) -> Option<ShakeCatalogVariant> {
    let direction = key
        .strip_prefix("effects/shake__GILDER_CATALOG_DIRECTION_")?
        .parse::<u8>()
        .ok()?;
    (direction <= 2).then_some(ShakeCatalogVariant { direction })
}

pub(super) fn resolver_source(variants: &[ShakeCatalogVariant]) -> String {
    let keys = variants
        .iter()
        .copied()
        .map(catalog_key)
        .map(|key| format!("    {key:?},\n"))
        .collect::<String>();
    format!(
        r#"
const SHAKE_CATALOG_KEYS: [&str; {count}] = [
{keys}];

fn shake_catalog_key_for_semantic_key(key: &str) -> Option<&'static str> {{
    let suffix = key.strip_prefix("effects/shake__")?;
    let mut slots = None;
    let mut direction = 0u8;
    let mut seen_slots = false;
    let mut seen_direction = false;
    for part in suffix.split("__") {{
        if let Some(value) = part.strip_prefix("SLOTS_") {{
            if seen_slots {{
                return None;
            }}
            slots = u32::from_str_radix(value, 16).ok();
            seen_slots = true;
        }} else if let Some(value) = part.strip_prefix("DIRECTION_") {{
            if seen_direction {{
                return None;
            }}
            direction = value.parse().ok()?;
            seen_direction = true;
        }} else {{
            return None;
        }}
    }}
    // Texture2 has an authored util/black default even when TIMEOFFSET=0.
    // O2 removes its sample, not its material slot identity.
    if slots? != 0x7 || direction > 2 {{
        return None;
    }}
    SHAKE_CATALOG_KEYS.get(usize::from(direction)).copied()
}}

"#,
        count = variants.len(),
    )
}

pub(super) fn vertex_source() -> String {
    r#"struct ShakeMaterialData
{
    float4 timeSpeedStrengthUnused;
    float4 boundsFriction;
    float4 texture1Resolution;
};

cbuffer ShakeMaterial : register(b3)
{
    ShakeMaterialData material;
}

struct ShakeVertexInput
{
    uint vertexId : SV_VertexID;
};

struct ShakeVertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float2 bounds : TEXCOORD1;
};

[[shader("vertex")]]
ShakeVertexOutput main(ShakeVertexInput input)
{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    float2 uv = position * 0.5 + 0.5;
    ShakeVertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord.xy = uv;
    output.texCoord.zw =
        uv * material.texture1Resolution.zw / material.texture1Resolution.xy;
    output.bounds = float2(
        material.boundsFriction.x,
        1.0 / (material.boundsFriction.y - material.boundsFriction.x));
    return output;
}
"#
    .to_owned()
}

pub(super) fn fragment_source(variant: ShakeCatalogVariant) -> String {
    let direction = match variant.direction {
        0 => "    offset = offset * 2.0 - 1.0;\n",
        1 => "",
        2 => "    offset -= 1.0;\n",
        _ => unreachable!("validated Shake direction"),
    };
    format!(
        r#"struct ShakeMaterialData
{{
    float4 timeSpeedStrengthUnused;
    float4 boundsFriction;
    float4 texture1Resolution;
}};

cbuffer ShakeMaterial : register(b3)
{{
    ShakeMaterialData material;
}}

Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> flowTexture : register(t1);
SamplerState flowSampler : register(s1);

struct ShakeFragmentInput
{{
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float2 bounds : TEXCOORD1;
}};

[[shader("fragment")]]
float4 main(ShakeFragmentInput input) : SV_Target0
{{
    float2 flow = (flowTexture.Sample(flowSampler, input.texCoord.zw).rg - 0.498) * 2.0;
    float phase = material.timeSpeedStrengthUnused.y
        * material.timeSpeedStrengthUnused.x;
    float offset = sin(frac(phase / 6.283185307179586) * 6.283185307179586);
    offset = offset * 0.498 + 0.5;
    float base = step(0.0, cos(phase));
    offset = lerp(
        1.0 - pow(1.0 - offset, material.boundsFriction.z),
        pow(offset, material.boundsFriction.w),
        base);
    offset = saturate((offset - input.bounds.x) * input.bounds.y);
{direction}    float2 uvOffset = offset
        * material.timeSpeedStrengthUnused.z
        * material.timeSpeedStrengthUnused.z
        * flow;
    return sourceTexture.Sample(sourceSampler, input.texCoord.xy + uvOffset);
}}
"#
    )
}
