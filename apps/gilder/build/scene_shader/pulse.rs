//! Native-Slang catalog stages for Wallpaper Engine's installed Pulse effect.
//!
//! References:
//! - `reverse-engineered/gilder/shaders/effects/pulse.{vert,frag}`
//! - `reverse-engineered/gilder/shaders/common_blending.h`
//! - `reverse-engineered/gilder/docs/blending-modes.md`

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PulseCatalogVariant {
    pub(super) audio_processing: u8,
    pub(super) mask_enabled: bool,
    pub(super) blend_mode: u8,
}

pub(super) fn catalog_variants() -> impl Iterator<Item = PulseCatalogVariant> {
    (0..=3).flat_map(|audio_processing| {
        [false, true].into_iter().flat_map(move |mask_enabled| {
            (0..=32).map(move |blend_mode| PulseCatalogVariant {
                audio_processing,
                mask_enabled,
                blend_mode,
            })
        })
    })
}

pub(super) fn catalog_key(variant: PulseCatalogVariant) -> String {
    format!(
        "effects/pulse__GILDER_CATALOG_AUDIO_{}__MASK_{}__BLENDMODE_{}",
        variant.audio_processing,
        u8::from(variant.mask_enabled),
        variant.blend_mode,
    )
}

pub(super) fn variant_from_catalog_key(key: &str) -> Option<PulseCatalogVariant> {
    let mut parts = key.split("__");
    (parts.next()? == "effects/pulse").then_some(())?;
    let audio_processing = parts
        .next()?
        .strip_prefix("GILDER_CATALOG_AUDIO_")?
        .parse::<u8>()
        .ok()?;
    let mask_enabled = match parts.next()?.strip_prefix("MASK_")? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let blend_mode = parts
        .next()?
        .strip_prefix("BLENDMODE_")?
        .parse::<u8>()
        .ok()?;
    (parts.next().is_none() && audio_processing <= 3 && blend_mode <= 32).then_some(
        PulseCatalogVariant {
            audio_processing,
            mask_enabled,
            blend_mode,
        },
    )
}

pub(super) fn resolver_source(variants: &[PulseCatalogVariant]) -> String {
    let keys = variants
        .iter()
        .copied()
        .map(catalog_key)
        .map(|key| format!("    {key:?},\n"))
        .collect::<String>();
    format!(
        r#"
const PULSE_CATALOG_KEYS: [&str; {count}] = [
{keys}];

fn pulse_catalog_key_for_semantic_key(key: &str) -> Option<&'static str> {{
    let suffix = key.strip_prefix("effects/pulse__")?;
    let mut slots = None;
    let mut audio_processing = 0u8;
    let mut blend_mode = 9u8;
    let mut mask = None;
    let mut seen_slots = false;
    let mut seen_audio = false;
    let mut seen_blend = false;
    let mut seen_mask = false;
    let mut seen_alpha = false;
    let mut seen_color = false;

    for part in suffix.split("__") {{
        if let Some(value) = part.strip_prefix("SLOTS_") {{
            if seen_slots {{
                return None;
            }}
            slots = u32::from_str_radix(value, 16).ok();
            seen_slots = true;
        }} else if let Some(value) = part.strip_prefix("AUDIOPROCESSING_") {{
            if seen_audio {{
                return None;
            }}
            audio_processing = value.parse().ok()?;
            seen_audio = true;
        }} else if let Some(value) = part.strip_prefix("BLENDMODE_") {{
            if seen_blend {{
                return None;
            }}
            blend_mode = value.parse().ok()?;
            seen_blend = true;
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
        }} else if let Some(value) = part.strip_prefix("PULSEALPHA_") {{
            if seen_alpha || !matches!(value, "0" | "1") {{
                return None;
            }}
            seen_alpha = true;
        }} else if let Some(value) = part.strip_prefix("PULSECOLOR_") {{
            if seen_color || !matches!(value, "0" | "1") {{
                return None;
            }}
            seen_color = true;
        }} else {{
            return None;
        }}
    }}

    let slots = slots?;
    if audio_processing > 3 || blend_mode > 32 {{
        return None;
    }}
    let mask = mask.unwrap_or(slots & 0x4 != 0);
    // The installed shader declares its util/noise default unconditionally.
    // AUDIOPROCESSING specializes the sample away, but does not remove the
    // authored material binding or its semantic slot-mask identity.
    let expected_slots = 1 | 2 | if mask {{ 4 }} else {{ 0 }};
    if slots != expected_slots {{
        return None;
    }}
    let index = ((usize::from(audio_processing) * 2 + usize::from(u8::from(mask))) * 33)
        + usize::from(blend_mode);
    PULSE_CATALOG_KEYS.get(index).copied()
}}

"#,
        count = variants.len(),
    )
}

pub(super) fn fullscreen_vertex_source(variant: PulseCatalogVariant) -> String {
    let pulse = vertex_pulse_expression(variant.audio_processing);
    let mask_uv = mask_uv_statement(variant.mask_enabled, "texCoord");
    let audio_helpers = audio_response_helpers(variant.audio_processing);
    let material = (variant.audio_processing != 0 || variant.mask_enabled)
        .then_some(material_source())
        .unwrap_or_default();
    format!(
        r#"{material}{audio_helpers}
struct PulseFullscreenVertexInput
{{
    uint vertexId : SV_VertexID;
}};

struct PulseVertexOutput
{{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float pulse : TEXCOORD1;
}};

[[shader("vertex")]]
PulseVertexOutput main(PulseFullscreenVertexInput input)
{{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    float2 texCoord = position * 0.5 + 0.5;
    PulseVertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord = float4(texCoord, texCoord);
{mask_uv}
    output.pulse = {pulse};
    return output;
}}
"#,
        material = material,
        audio_helpers = audio_helpers,
    )
}

pub(super) fn object_mesh_vertex_source(variant: PulseCatalogVariant) -> String {
    let pulse = vertex_pulse_expression(variant.audio_processing);
    let mask_uv = mask_uv_statement(variant.mask_enabled, "input.texCoord");
    let audio_helpers = audio_response_helpers(variant.audio_processing);
    let material = (variant.audio_processing != 0 || variant.mask_enabled)
        .then_some(material_source())
        .unwrap_or_default();
    format!(
        r#"{material}{audio_helpers}
struct SceneDrawTransformData
{{
    float4 modelViewProjectionMatrix[4];
}};

cbuffer SceneDrawTransform : register(b2)
{{
    SceneDrawTransformData drawTransform;
}}

struct PulseObjectVertexInput
{{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 texCoord : TEXCOORD0;
}};

struct PulseVertexOutput
{{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float pulse : TEXCOORD1;
}};

[[shader("vertex")]]
PulseVertexOutput main(PulseObjectVertexInput input)
{{
    float4 localPosition = float4(input.position, 0.0, 1.0);
    PulseVertexOutput output;
    output.position = float4(
        dot(drawTransform.modelViewProjectionMatrix[0], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[1], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[2], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[3], localPosition));
    output.texCoord = float4(input.texCoord, input.texCoord);
{mask_uv}
    output.pulse = {pulse};
    return output;
}}
"#,
        material = material,
        audio_helpers = audio_helpers,
    )
}

pub(super) fn fragment_source(variant: PulseCatalogVariant) -> String {
    let noise_resources = (variant.audio_processing == 0).then_some(
        "Texture2D<float4> noiseTexture : register(t1);\nSamplerState noiseSampler : register(s1);\n",
    );
    let mask_resources = variant.mask_enabled.then_some(
        "Texture2D<float4> maskTexture : register(t2);\nSamplerState maskSampler : register(s2);\n",
    );
    let pulse = fragment_pulse_expression(variant.audio_processing);
    let color = "if (material.toggles.y != 0.0) {\n        albedo.rgb = blendPulse(albedo.rgb * material.tintColor1.xyz, albedo.rgb * material.tintColor2.xyz, pulse);\n    }";
    let alpha = "if (material.toggles.x != 0.0) {\n        albedo.w *= pulse;\n    }";
    let mask = variant.mask_enabled.then_some(
        "float mask = maskTexture.Sample(maskSampler, input.texCoord.zw).x;\n    albedo = lerp(sample, albedo, mask);",
    );
    format!(
        r#"Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
{noise_resources}{mask_resources}{material}
struct PulseFragmentInput
{{
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float pulse : TEXCOORD1;
}};

{blend_helpers}
[[shader("fragment")]]
float4 main(PulseFragmentInput input) : SV_Target0
{{
    float4 sample = sourceTexture.Sample(sourceSampler, input.texCoord.xy);
    float4 albedo = sample;
    {pulse}
    {color}
    {alpha}
    {mask}
    return float4(max(float3(0.0), albedo.xyz), albedo.w);
}}
"#,
        noise_resources = noise_resources.unwrap_or_default(),
        mask_resources = mask_resources.unwrap_or_default(),
        material = material_source(),
        pulse = pulse,
        color = color,
        alpha = alpha,
        mask = mask.unwrap_or_default(),
        blend_helpers = blend_helpers(variant.blend_mode),
    )
}

fn material_source() -> &'static str {
    r#"struct PulseMaterialData
{
    float4 toggles;
    float4 timeSpeedPhaseAmount;
    float4 thresholdsPowerNoiseSpeed;
    float4 noiseAmountAudioFrequency;
    float4 audioBoundsMultiply;
    float4 tintColor1;
    float4 tintColor2;
    float4 texture2Resolution;
    float4 audioSpectrum16Left[4];
    float4 audioSpectrum16Right[4];
};

cbuffer PulseMaterial : register(b3)
{
    PulseMaterialData material;
}
"#
}

fn mask_uv_statement(mask_enabled: bool, tex_coord: &str) -> String {
    if mask_enabled {
        format!(
            "    output.texCoord.zw = {tex_coord} * material.texture2Resolution.zw / material.texture2Resolution.xy;"
        )
    } else {
        String::new()
    }
}

fn vertex_pulse_expression(audio_processing: u8) -> &'static str {
    match audio_processing {
        0 => "0.0",
        1..=3 => "pulseAudioResponse()",
        _ => unreachable!("validated Pulse audio variant"),
    }
}

fn fragment_pulse_expression(audio_processing: u8) -> &'static str {
    if audio_processing == 0 {
        r#"float pulse = smoothstep(
        material.thresholdsPowerNoiseSpeed.x,
        material.thresholdsPowerNoiseSpeed.y,
        sin(material.timeSpeedPhaseAmount.x * material.timeSpeedPhaseAmount.y
            + (material.timeSpeedPhaseAmount.z - 1.57079632679)) * 0.5 + 0.5)
        * material.timeSpeedPhaseAmount.w;
    float noise = noiseTexture.Sample(
        noiseSampler,
        float2(material.timeSpeedPhaseAmount.x * 0.08333333,
            material.timeSpeedPhaseAmount.x * 0.02777777)
            * material.thresholdsPowerNoiseSpeed.w).x
        * material.noiseAmountAudioFrequency.x;
    pulse = pow(pulse + noise, material.thresholdsPowerNoiseSpeed.z);"#
    } else {
        "float pulse = input.pulse;"
    }
}

fn audio_response_helpers(audio_processing: u8) -> String {
    let response = match audio_processing {
        1 => "audioResponse += pulseAudioBandLeft(band);",
        2 => "audioResponse += pulseAudioBandRight(band);",
        3 => {
            "audioResponse += pulseAudioBandLeft(band);\n        audioResponse += pulseAudioBandRight(band);"
        }
        _ => return String::new(),
    };
    let denominator = if audio_processing == 3 { "2.0" } else { "1.0" };
    format!(
        r#"float pulseAudioBandLeft(int band)
{{
    int group = band / 4;
    return material.audioSpectrum16Left[group][band - group * 4];
}}

float pulseAudioBandRight(int band)
{{
    int group = band / 4;
    return material.audioSpectrum16Right[group][band - group * 4];
}}

float pulseAudioResponse()
{{
    float audioResponse = 0.0;
    for (int band = int(material.noiseAmountAudioFrequency.y);
        band <= int(material.noiseAmountAudioFrequency.z);
        ++band)
    {{
        {response}
    }}
    audioResponse /= (material.noiseAmountAudioFrequency.z
        - material.noiseAmountAudioFrequency.y + 1.0) * {denominator};
    audioResponse = smoothstep(
        material.audioBoundsMultiply.x,
        material.audioBoundsMultiply.y,
        audioResponse);
    return saturate(pow(audioResponse, material.noiseAmountAudioFrequency.w))
        * material.audioBoundsMultiply.z;
}}
"#
    )
}

fn blend_helpers(mode: u8) -> String {
    let blend = match mode {
        0 => "return lerp(base, blend, opacity);",
        1 => "return lerp(base, min(base, blend), opacity);",
        2 => "return lerp(base, base * blend, opacity);",
        3 => "return lerp(base, colorBurn(base, blend), opacity);",
        4 | 20 => "return lerp(base, max(base + blend - 1.0, 0.0), opacity);",
        5 => "return min(base, blend);",
        6 => "return lerp(base, max(base, blend), opacity);",
        7 => "return lerp(base, screen(base, blend), opacity);",
        8 => "return lerp(base, colorDodge(base, blend), opacity);",
        9 => "return lerp(base, min(base + blend, 1.0), opacity);",
        10 => "return max(base, blend);",
        11 => "return lerp(base, overlay(base, blend), opacity);",
        12 => "return lerp(base, softLight(base, blend), opacity);",
        13 => "return lerp(base, overlay(blend, base), opacity);",
        14 => "return lerp(base, vividLight(base, blend), opacity);",
        15 => "return lerp(base, linearLight(base, blend), opacity);",
        16 => "return lerp(base, pinLight(base, blend), opacity);",
        17 => "return lerp(base, hardMix(base, blend), opacity);",
        18 => "return lerp(base, abs(base - blend), opacity);",
        19 => "return lerp(base, base + blend - 2.0 * base * blend, opacity);",
        21 => "return lerp(base, reflectBlend(base, blend), opacity);",
        22 => "return lerp(base, reflectBlend(blend, base), opacity);",
        23 => "return lerp(base, min(base, blend) - max(base, blend) + 1.0, opacity);",
        24 => "return lerp(base, (base + blend) * 0.5, opacity);",
        25 => "return lerp(base, 1.0 - abs(1.0 - base - blend), opacity);",
        26 => "return lerp(base, blendHue(base, blend), opacity);",
        27 => "return lerp(base, blendSaturation(base, blend), opacity);",
        28 => "return lerp(base, blendColor(base, blend), opacity);",
        29 => "return lerp(base, blendLuminosity(base, blend), opacity);",
        30 => "return lerp(base, float3(max(base.x, max(base.y, base.z))) * blend, opacity);",
        31 => "return base + blend * opacity;",
        32 => "return lerp(base, base + base * blend, opacity);",
        _ => unreachable!("validated Pulse blend variant"),
    };
    let scalar_helpers = matches!(mode, 3 | 7 | 8 | 11..=17 | 21 | 22);
    let hsl_helpers = matches!(mode, 26..=29);
    format!(
        r#"{scalar_helpers}{hsl_helpers}
float3 blendPulse(float3 base, float3 blend, float opacity)
{{
    {blend}
}}
"#,
        scalar_helpers = scalar_helpers
            .then_some(SCALAR_BLEND_HELPERS)
            .unwrap_or_default(),
        hsl_helpers = hsl_helpers.then_some(HSL_BLEND_HELPERS).unwrap_or_default(),
    )
}

const SCALAR_BLEND_HELPERS: &str = r#"float colorBurnComponent(float base, float blend)
{
    return blend == 0.0 ? blend : max(1.0 - (1.0 - base) / blend, 0.0);
}

float colorDodgeComponent(float base, float blend)
{
    return blend == 1.0 ? blend : min(base / (1.0 - blend), 1.0);
}

float overlayComponent(float base, float blend)
{
    return base < 0.5
        ? 2.0 * base * blend
        : 1.0 - 2.0 * (1.0 - base) * (1.0 - blend);
}

float softLightComponent(float base, float blend)
{
    return blend < 0.5
        ? 2.0 * base * blend + base * base * (1.0 - 2.0 * blend)
        : sqrt(base) * (2.0 * blend - 1.0) + 2.0 * base * (1.0 - blend);
}

float3 colorBurn(float3 base, float3 blend)
{
    return float3(
        colorBurnComponent(base.x, blend.x),
        colorBurnComponent(base.y, blend.y),
        colorBurnComponent(base.z, blend.z));
}

float3 colorDodge(float3 base, float3 blend)
{
    return float3(
        colorDodgeComponent(base.x, blend.x),
        colorDodgeComponent(base.y, blend.y),
        colorDodgeComponent(base.z, blend.z));
}

float3 screen(float3 base, float3 blend)
{
    return 1.0 - (1.0 - base) * (1.0 - blend);
}

float3 overlay(float3 base, float3 blend)
{
    return float3(
        overlayComponent(base.x, blend.x),
        overlayComponent(base.y, blend.y),
        overlayComponent(base.z, blend.z));
}

float3 softLight(float3 base, float3 blend)
{
    return float3(
        softLightComponent(base.x, blend.x),
        softLightComponent(base.y, blend.y),
        softLightComponent(base.z, blend.z));
}

float3 vividLight(float3 base, float3 blend)
{
    return float3(
        blend.x < 0.5
            ? colorBurnComponent(base.x, 2.0 * blend.x)
            : colorDodgeComponent(base.x, 2.0 * (blend.x - 0.5)),
        blend.y < 0.5
            ? colorBurnComponent(base.y, 2.0 * blend.y)
            : colorDodgeComponent(base.y, 2.0 * (blend.y - 0.5)),
        blend.z < 0.5
            ? colorBurnComponent(base.z, 2.0 * blend.z)
            : colorDodgeComponent(base.z, 2.0 * (blend.z - 0.5)));
}

float3 linearLight(float3 base, float3 blend)
{
    return float3(
        blend.x < 0.5 ? max(base.x + 2.0 * blend.x - 1.0, 0.0) : base.x + 2.0 * (blend.x - 0.5),
        blend.y < 0.5 ? max(base.y + 2.0 * blend.y - 1.0, 0.0) : base.y + 2.0 * (blend.y - 0.5),
        blend.z < 0.5 ? max(base.z + 2.0 * blend.z - 1.0, 0.0) : base.z + 2.0 * (blend.z - 0.5));
}

float3 pinLight(float3 base, float3 blend)
{
    return float3(
        blend.x < 0.5 ? min(base.x, 2.0 * blend.x) : max(base.x, 2.0 * (blend.x - 0.5)),
        blend.y < 0.5 ? min(base.y, 2.0 * blend.y) : max(base.y, 2.0 * (blend.y - 0.5)),
        blend.z < 0.5 ? min(base.z, 2.0 * blend.z) : max(base.z, 2.0 * (blend.z - 0.5)));
}

float3 hardMix(float3 base, float3 blend)
{
    float3 vivid = vividLight(base, blend);
    return float3(vivid.x < 0.5 ? 0.0 : 1.0, vivid.y < 0.5 ? 0.0 : 1.0, vivid.z < 0.5 ? 0.0 : 1.0);
}

float3 reflectBlend(float3 base, float3 blend)
{
    return float3(
        blend.x == 1.0 ? blend.x : min(base.x * base.x / (1.0 - blend.x), 1.0),
        blend.y == 1.0 ? blend.y : min(base.y * base.y / (1.0 - blend.y), 1.0),
        blend.z == 1.0 ? blend.z : min(base.z * base.z / (1.0 - blend.z), 1.0));
}
"#;

const HSL_BLEND_HELPERS: &str = r#"float3 rgbToHsl(float3 color)
{
    float minimum = min(min(color.x, color.y), color.z);
    float maximum = max(max(color.x, color.y), color.z);
    float delta = maximum - minimum;
    float3 hsl;
    hsl.z = (maximum + minimum) * 0.5;
    if (delta == 0.0)
    {
        hsl.x = 0.0;
        hsl.y = 0.0;
        return hsl;
    }
    hsl.y = hsl.z < 0.5 ? delta / (maximum + minimum) : delta / (2.0 - maximum - minimum);
    float deltaR = (((maximum - color.x) / 6.0) + (delta * 0.5)) / delta;
    float deltaG = (((maximum - color.y) / 6.0) + (delta * 0.5)) / delta;
    float deltaB = (((maximum - color.z) / 6.0) + (delta * 0.5)) / delta;
    hsl.x = color.x == maximum
        ? deltaB - deltaG
        : color.y == maximum ? (1.0 / 3.0) + deltaR - deltaB : (2.0 / 3.0) + deltaG - deltaR;
    if (hsl.x < 0.0)
    {
        hsl.x += 1.0;
    }
    else if (hsl.x > 1.0)
    {
        hsl.x -= 1.0;
    }
    return hsl;
}

float hueToRgb(float first, float second, float hue)
{
    if (hue < 0.0)
    {
        hue += 1.0;
    }
    else if (hue > 1.0)
    {
        hue -= 1.0;
    }
    if (6.0 * hue < 1.0)
    {
        return first + (second - first) * 6.0 * hue;
    }
    if (2.0 * hue < 1.0)
    {
        return second;
    }
    if (3.0 * hue < 2.0)
    {
        return first + (second - first) * ((2.0 / 3.0) - hue) * 6.0;
    }
    return first;
}

float3 hslToRgb(float3 hsl)
{
    if (hsl.y == 0.0)
    {
        return float3(hsl.z);
    }
    float second = hsl.z < 0.5 ? hsl.z * (1.0 + hsl.y) : (hsl.z + hsl.y) - hsl.y * hsl.z;
    float first = 2.0 * hsl.z - second;
    return float3(
        hueToRgb(first, second, hsl.x + 1.0 / 3.0),
        hueToRgb(first, second, hsl.x),
        hueToRgb(first, second, hsl.x - 1.0 / 3.0));
}

float3 blendHue(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(rgbToHsl(blend).x, baseHsl.y, baseHsl.z));
}

float3 blendSaturation(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(baseHsl.x, rgbToHsl(blend).y, baseHsl.z));
}

float3 blendColor(float3 base, float3 blend)
{
    float3 blendHsl = rgbToHsl(blend);
    return hslToRgb(float3(blendHsl.x, blendHsl.y, rgbToHsl(base).z));
}

float3 blendLuminosity(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(baseHsl.x, baseHsl.y, rgbToHsl(blend).z));
}
"#;

const _: () = {
    assert!(std::mem::size_of::<PulseCatalogVariant>() >= 3);
};
