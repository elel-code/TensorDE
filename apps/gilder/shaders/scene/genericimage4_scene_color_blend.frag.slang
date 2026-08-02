Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> sceneSnapshot : register(t4);
SamplerState sceneSampler : register(s4);

struct SceneColorBlendMaterialData
{
    float4 color;
    float4 blendMode;
};

cbuffer SceneColorBlendMaterial : register(b3)
{
    SceneColorBlendMaterialData material;
}

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
    [[vk::location(1)]] float3 screenPosition : TEXCOORD1;
};

float colorBurn(float base, float blend)
{
    return blend == 0.0 ? blend : max(1.0 - (1.0 - base) / blend, 0.0);
}

float colorDodge(float base, float blend)
{
    return blend == 1.0 ? blend : min(base / (1.0 - blend), 1.0);
}

float overlay(float base, float blend)
{
    return base < 0.5
        ? 2.0 * base * blend
        : 1.0 - 2.0 * (1.0 - base) * (1.0 - blend);
}

float softLight(float base, float blend)
{
    return blend < 0.5
        ? 2.0 * base * blend + base * base * (1.0 - 2.0 * blend)
        : sqrt(base) * (2.0 * blend - 1.0) + 2.0 * base * (1.0 - blend);
}

float vividLight(float base, float blend)
{
    return blend < 0.5
        ? colorBurn(base, 2.0 * blend)
        : colorDodge(base, 2.0 * (blend - 0.5));
}

float linearLight(float base, float blend)
{
    return blend < 0.5
        ? max(base + 2.0 * blend - 1.0, 0.0)
        : base + 2.0 * (blend - 0.5);
}

float pinLight(float base, float blend)
{
    return blend < 0.5 ? min(base, 2.0 * blend) : max(base, 2.0 * (blend - 0.5));
}

float reflectBlend(float base, float blend)
{
    return blend == 1.0 ? blend : min(base * base / (1.0 - blend), 1.0);
}

float3 componentColorBurn(float3 base, float3 blend)
{
    return float3(
        colorBurn(base.r, blend.r),
        colorBurn(base.g, blend.g),
        colorBurn(base.b, blend.b));
}

float3 componentColorDodge(float3 base, float3 blend)
{
    return float3(
        colorDodge(base.r, blend.r),
        colorDodge(base.g, blend.g),
        colorDodge(base.b, blend.b));
}

float3 componentOverlay(float3 base, float3 blend)
{
    return float3(
        overlay(base.r, blend.r),
        overlay(base.g, blend.g),
        overlay(base.b, blend.b));
}

float3 componentSoftLight(float3 base, float3 blend)
{
    return float3(
        softLight(base.r, blend.r),
        softLight(base.g, blend.g),
        softLight(base.b, blend.b));
}

float3 componentVividLight(float3 base, float3 blend)
{
    return float3(
        vividLight(base.r, blend.r),
        vividLight(base.g, blend.g),
        vividLight(base.b, blend.b));
}

float3 componentLinearLight(float3 base, float3 blend)
{
    return float3(
        linearLight(base.r, blend.r),
        linearLight(base.g, blend.g),
        linearLight(base.b, blend.b));
}

float3 componentPinLight(float3 base, float3 blend)
{
    return float3(
        pinLight(base.r, blend.r),
        pinLight(base.g, blend.g),
        pinLight(base.b, blend.b));
}

float3 componentHardMix(float3 base, float3 blend)
{
    float3 vivid = componentVividLight(base, blend);
    return float3(
        vivid.r < 0.5 ? 0.0 : 1.0,
        vivid.g < 0.5 ? 0.0 : 1.0,
        vivid.b < 0.5 ? 0.0 : 1.0);
}

float3 componentReflect(float3 base, float3 blend)
{
    return float3(
        reflectBlend(base.r, blend.r),
        reflectBlend(base.g, blend.g),
        reflectBlend(base.b, blend.b));
}

float3 rgbToHsl(float3 color)
{
    float minimum = min(min(color.r, color.g), color.b);
    float maximum = max(max(color.r, color.g), color.b);
    float delta = maximum - minimum;
    float3 hsl = float3(0.0, 0.0, (maximum + minimum) * 0.5);
    if (delta != 0.0)
    {
        hsl.y = hsl.z < 0.5
            ? delta / (maximum + minimum)
            : delta / (2.0 - maximum - minimum);
        float deltaR = (((maximum - color.r) / 6.0) + delta * 0.5) / delta;
        float deltaG = (((maximum - color.g) / 6.0) + delta * 0.5) / delta;
        float deltaB = (((maximum - color.b) / 6.0) + delta * 0.5) / delta;
        if (color.r == maximum)
            hsl.x = deltaB - deltaG;
        else if (color.g == maximum)
            hsl.x = 1.0 / 3.0 + deltaR - deltaB;
        else
            hsl.x = 2.0 / 3.0 + deltaG - deltaR;
        if (hsl.x < 0.0)
            hsl.x += 1.0;
        else if (hsl.x > 1.0)
            hsl.x -= 1.0;
    }
    return hsl;
}

float hueToRgb(float first, float second, float hue)
{
    if (hue < 0.0)
        hue += 1.0;
    else if (hue > 1.0)
        hue -= 1.0;
    if (6.0 * hue < 1.0)
        return first + (second - first) * 6.0 * hue;
    if (2.0 * hue < 1.0)
        return second;
    if (3.0 * hue < 2.0)
        return first + (second - first) * (2.0 / 3.0 - hue) * 6.0;
    return first;
}

float3 hslToRgb(float3 hsl)
{
    if (hsl.y == 0.0)
        return float3(hsl.z, hsl.z, hsl.z);
    float second = hsl.z < 0.5
        ? hsl.z * (1.0 + hsl.y)
        : hsl.z + hsl.y - hsl.y * hsl.z;
    float first = 2.0 * hsl.z - second;
    return float3(
        hueToRgb(first, second, hsl.x + 1.0 / 3.0),
        hueToRgb(first, second, hsl.x),
        hueToRgb(first, second, hsl.x - 1.0 / 3.0));
}

float3 hueBlend(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(rgbToHsl(blend).r, baseHsl.g, baseHsl.b));
}

float3 saturationBlend(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(baseHsl.r, rgbToHsl(blend).g, baseHsl.b));
}

float3 colorBlend(float3 base, float3 blend)
{
    float3 blendHsl = rgbToHsl(blend);
    return hslToRgb(float3(blendHsl.r, blendHsl.g, rgbToHsl(base).b));
}

float3 luminosityBlend(float3 base, float3 blend)
{
    float3 baseHsl = rgbToHsl(base);
    return hslToRgb(float3(baseHsl.r, baseHsl.g, rgbToHsl(blend).b));
}

float3 applyBlend(int mode, float3 base, float3 blend, float opacity)
{
    float3 result = blend;
    switch (mode)
    {
        case 1: result = min(base, blend); break;
        case 2: result = base * blend; break;
        case 3: result = componentColorBurn(base, blend); break;
        case 4: result = max(base + blend - 1.0, 0.0); break;
        case 5: return min(base, blend);
        case 6: result = max(base, blend); break;
        case 7: result = 1.0 - (1.0 - base) * (1.0 - blend); break;
        case 8: result = componentColorDodge(base, blend); break;
        case 9: result = min(base + blend, 1.0); break;
        case 10: return max(base, blend);
        case 11: result = componentOverlay(base, blend); break;
        case 12: result = componentSoftLight(base, blend); break;
        case 13: result = componentOverlay(blend, base); break;
        case 14: result = componentVividLight(base, blend); break;
        case 15: result = componentLinearLight(base, blend); break;
        case 16: result = componentPinLight(base, blend); break;
        case 17: result = componentHardMix(base, blend); break;
        case 18: result = abs(base - blend); break;
        case 19: result = base + blend - 2.0 * base * blend; break;
        case 20: result = max(base + blend - 1.0, 0.0); break;
        case 21: result = componentReflect(base, blend); break;
        case 22: result = componentReflect(blend, base); break;
        case 23: result = min(base, blend) - max(base, blend) + 1.0; break;
        case 24: result = (base + blend) * 0.5; break;
        case 25: result = 1.0 - abs(1.0 - base - blend); break;
        case 26: result = hueBlend(base, blend); break;
        case 27: result = saturationBlend(base, blend); break;
        case 28: result = colorBlend(base, blend); break;
        case 29: result = luminosityBlend(base, blend); break;
        case 30: result = max(max(base.r, base.g), base.b) * blend; break;
        case 31: return base + blend * opacity;
        case 32: return lerp(base, base + base * blend, opacity);
        default: break;
    }
    return lerp(base, result, opacity);
}

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float4 albedo = sourceTexture.Sample(sourceSampler, input.texCoord) * material.color;
    float2 screenCoord = input.screenPosition.xy / input.screenPosition.z * 0.5 + 0.5;
    float4 screen = sceneSnapshot.Sample(sceneSampler, screenCoord);
    int mode = int(round(material.blendMode.x));
    return float4(applyBlend(mode, screen.rgb, albedo.rgb, albedo.a), screen.a);
}
