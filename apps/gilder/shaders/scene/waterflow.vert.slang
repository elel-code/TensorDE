struct SceneDrawTransformData
{
    float4 modelViewProjectionMatrix[4];
};

cbuffer SceneDrawTransform : register(b2)
{
    SceneDrawTransformData drawTransform;
}

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

struct VertexInput
{
    uint vertexId : SV_VertexID;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float4 cycles : TEXCOORD1;
    [[vk::location(2)]] float2 blend : TEXCOORD2;
};

[[shader("vertex")]]
VertexOutput main(VertexInput input)
{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    float2 screenUv = position * 0.5 + 0.5;
    float2 objectUv = float2(
        dot(drawTransform.modelViewProjectionMatrix[0].xyz, float3(screenUv, 1.0)),
        dot(drawTransform.modelViewProjectionMatrix[1].xyz, float3(screenUv, 1.0)));
    float4 cycles = frac(
        material.timeSpeedFeatherStrength.x * material.timeSpeedFeatherStrength.y
        + float4(0.0, 0.5, 0.25, 0.75));
    float2 phase = 2.0 * abs(float2(cycles.x, cycles.z) - float2(0.5, 0.5));
    float feather = material.timeSpeedFeatherStrength.z;
    float2 smoothRange = float2(0.5 - feather, 0.5 + feather);

    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord = float4(
        objectUv,
        objectUv * material.texture1Resolution.zw / material.texture1Resolution.xy);
    output.cycles = cycles - float4(0.5, 0.5, 0.5, 0.5);
    output.blend = smoothstep(smoothRange.x, smoothRange.y, phase);
    return output;
}
