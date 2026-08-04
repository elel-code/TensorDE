// Resident sampled icons with optional rounded-corner coverage.
//
// The texture and sampler are selected through direct descriptor-heap
// indices pushed per batch; see `VulkanIconRenderer::draw_batches`.

struct IconPush
{
    uint imageIndex;
    uint samplerIndex;
};

[[vk::push_constant]] ConstantBuffer<IconPush> push;

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 uv : UV;
    [[vk::location(2)]] float4 roundingBounds : BOUNDS;
    [[vk::location(3)]] float2 radiusAlpha : STYLE;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : UV;
    [[vk::location(1)]] nointerpolation float4 roundingBounds : BOUNDS;
    [[vk::location(2)]] nointerpolation float2 radiusAlpha : STYLE;
};

[shader("vertex")]
VertexOutput vertexMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.roundingBounds = input.roundingBounds;
    output.radiusAlpha = input.radiusAlpha;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    Texture2D<float4> icon = DescriptorHandle<Texture2D<float4>>(push.imageIndex);
    SamplerState iconSampler = DescriptorHandle<SamplerState>(push.samplerIndex);
    float4 color = icon.Sample(iconSampler, input.uv);
    float coverage = 1.0;
    float4 bounds = input.roundingBounds;
    if (input.radiusAlpha.x > 0.0 && bounds.z > bounds.x && bounds.w > bounds.y)
    {
        float2 extent = bounds.zw - bounds.xy;
        float radius = min(extent.x, extent.y) * input.radiusAlpha.x;
        float2 center = clamp(input.uv, bounds.xy + radius, bounds.zw - radius);
        float distance = length(input.uv - center) - radius;
        float antialias = max(fwidth(distance) * 0.5, 0.00001);
        coverage = 1.0 - smoothstep(-antialias, antialias, distance);
    }
    return color * (coverage * input.radiusAlpha.y);
}
