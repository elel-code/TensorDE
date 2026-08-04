Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);
Texture2D<float4> flowTexture : register(t1);
SamplerState flowSampler : register(s1);
Texture2D<float4> phaseTexture : register(t2);
SamplerState phaseSampler : register(s2);

struct WaterFlowMaterialData
{
    float4 timeSpeedFeatherStrength;
    float4 phaseScale;
    float4 texture1Resolution;
    float4 unused;
};

cbuffer WaterFlowMaterial : register(b3)
{
    WaterFlowMaterialData material;
}

struct FragmentInput
{
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float4 cycles : TEXCOORD1;
    [[vk::location(2)]] float2 blend : TEXCOORD2;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float flowPhase = phaseTexture.Sample(
        phaseSampler,
        input.texCoord.xy * material.phaseScale.x).r;
    float2 flowMask = (flowTexture.Sample(flowSampler, input.texCoord.zw).rg
        - float2(0.498, 0.498)) * 2.0;
    float flowAmount = length(flowMask);
    float strength = material.timeSpeedFeatherStrength.w * 0.1;
    float4 flowUvOffset = float4(
        flowMask.x,
        flowMask.y,
        flowMask.x,
        flowMask.y) * strength * float4(
            input.cycles.x,
            input.cycles.x,
            input.cycles.y,
            input.cycles.y);
    float4 flowUvOffset2 = float4(
        flowMask.x,
        flowMask.y,
        flowMask.x,
        flowMask.y) * strength * float4(
            input.cycles.z,
            input.cycles.z,
            input.cycles.w,
            input.cycles.w);
    float4 albedo = sourceTexture.Sample(sourceSampler, input.texCoord.xy);
    float4 first = lerp(
        sourceTexture.Sample(sourceSampler, input.texCoord.xy + flowUvOffset.xy),
        sourceTexture.Sample(sourceSampler, input.texCoord.xy + flowUvOffset.zw),
        input.blend.x);
    float4 second = lerp(
        sourceTexture.Sample(sourceSampler, input.texCoord.xy + flowUvOffset2.xy),
        sourceTexture.Sample(sourceSampler, input.texCoord.xy + flowUvOffset2.zw),
        input.blend.y);
    float4 flowed = lerp(first, second, smoothstep(0.2, 0.8, flowPhase));
    return lerp(albedo, flowed, flowAmount);
}
