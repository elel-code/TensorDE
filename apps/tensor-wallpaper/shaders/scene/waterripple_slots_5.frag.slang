Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> normalTexture : register(t2);
SamplerState normalSampler : register(s2);

struct WaterRippleMaterialData
{
    float4 timeAnimationScaleScroll;
    float4 directionStrengthAspectNormal;
    float4 maskFlags;
    float4 textureMaskResolution;
};

cbuffer WaterRippleMaterial : register(b3)
{
    WaterRippleMaterialData material;
}

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
};

float2 rotateVec2(float2 value, float radians)
{
    float2 cosineSine = float2(cos(radians), sin(radians));
    return float2(
        value.x * cosineSine.x - value.y * cosineSine.y,
        value.x * cosineSine.y + value.y * cosineSine.x);
}

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float2 texCoord = input.texCoord;
    float strength = material.directionStrengthAspectNormal.y;
    if (strength == 0.0)
    {
        return sourceTexture.Sample(sourceSampler, texCoord);
    }

    float2 scroll = rotateVec2(
        float2(0.0, 1.0),
        material.directionStrengthAspectNormal.x)
        * material.timeAnimationScaleScroll.w
        * material.timeAnimationScaleScroll.w
        * material.timeAnimationScaleScroll.x;
    float animation = material.timeAnimationScaleScroll.x
        * material.timeAnimationScaleScroll.y
        * material.timeAnimationScaleScroll.y;
    float4 ripple = float4(
        texCoord + animation + scroll,
        texCoord * 1.333 - animation + scroll)
        * material.timeAnimationScaleScroll.z;
    ripple.xz *= material.directionStrengthAspectNormal.z;
    ripple.yw *= material.maskFlags.w;
    float3 normal1 = normalTexture.Sample(normalSampler, ripple.xy).xyz * 2.0 - 1.0;
    float3 normal2 = normalTexture.Sample(normalSampler, ripple.zw).xyz * 2.0 - 1.0;
    float3 normal = normalize(float3(normal1.xy + normal2.xy, normal1.z));
    texCoord += normal.xy * strength * strength;
    return sourceTexture.Sample(sourceSampler, texCoord);
}
