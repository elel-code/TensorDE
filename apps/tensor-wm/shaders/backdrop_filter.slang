// Fixed-cost separable backdrop filter.
//
// Each pass samples one retained descriptor-heap image and overwrites one
// region-local color target. Horizontal and vertical directions share the
// same retained pipeline and differ only in push data.

struct FilterData
{
    uint descriptorIndex;
    uint samplerIndex;
    uint radius;
    uint horizontal;
    float2 inverseExtent;
    float2 uvScale;
};

[[vk::push_constant]] ConstantBuffer<FilterData> filter;

static const float2 kPositions[6] =
{
    float2(0.0, 0.0),
    float2(1.0, 0.0),
    float2(1.0, 1.0),
    float2(0.0, 0.0),
    float2(1.0, 1.0),
    float2(0.0, 1.0),
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 local = kPositions[vertexIndex];
    VertexOutput output;
    output.position = float4(local * 2.0 - 1.0, 0.0, 1.0);
    output.texCoord = local * filter.uvScale;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    Texture2D<float4> source =
        DescriptorHandle<Texture2D<float4>>(filter.descriptorIndex);
    SamplerState samplerState =
        DescriptorHandle<SamplerState>(filter.samplerIndex);
    float2 axis = filter.horizontal != 0
        ? float2(filter.inverseExtent.x, 0.0)
        : float2(0.0, filter.inverseExtent.y);
    float stepScale = max(float(filter.radius), 1.0) * 0.25;

    // Symmetric normalized nine-tap Gaussian approximation. Radius changes
    // spacing, not loop length, so frame cost remains bounded.
    float4 color = source.Sample(samplerState, input.texCoord) * 0.2270270270;
    color += source.Sample(samplerState, input.texCoord + axis * stepScale) * 0.1945945946;
    color += source.Sample(samplerState, input.texCoord - axis * stepScale) * 0.1945945946;
    color += source.Sample(samplerState, input.texCoord + axis * stepScale * 2.0) * 0.1216216216;
    color += source.Sample(samplerState, input.texCoord - axis * stepScale * 2.0) * 0.1216216216;
    color += source.Sample(samplerState, input.texCoord + axis * stepScale * 3.0) * 0.0540540541;
    color += source.Sample(samplerState, input.texCoord - axis * stepScale * 3.0) * 0.0540540541;
    color += source.Sample(samplerState, input.texCoord + axis * stepScale * 4.0) * 0.0162162162;
    color += source.Sample(samplerState, input.texCoord - axis * stepScale * 4.0) * 0.0162162162;
    return color;
}
