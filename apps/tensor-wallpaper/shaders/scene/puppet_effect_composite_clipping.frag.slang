Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> clippingMask : register(t8);
SamplerState clippingMaskSampler : register(s8);

struct PuppetEffectCompositeMaterialData
{
    float4 color;
    float4 roughnessMetallic;
    float4 specularTint;
};

cbuffer PuppetEffectCompositeMaterial : register(b3)
{
    PuppetEffectCompositeMaterialData material;
}

struct FragmentInput
{
    [[vk::location(0)]] float2 effectTexCoord : TEXCOORD0;
    [[vk::location(1)]] float boneAlpha : TEXCOORD1;
    [[vk::location(2)]] float3 clippingScreenPosition : TEXCOORD2;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float4 color = sourceTexture.Sample(sourceSampler, input.effectTexCoord) * material.color;
    float2 screenUv = (input.clippingScreenPosition.xy / input.clippingScreenPosition.z) * 0.5 + 0.5;
    color.a *= clippingMask.Sample(clippingMaskSampler, screenUv).r;
    color.a *= input.boneAlpha;
    return color;
}
