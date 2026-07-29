struct FocusRingData
{
    float4 destination;
    float4 color;
    float4 innerRect;
    float4 shape;
};

[[vk::push_constant]] ConstantBuffer<FocusRingData> ring;

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
    [[vk::location(0)]] float2 pixel : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 local = kPositions[vertexIndex];

    VertexOutput output;
    output.position = float4(
        ring.destination.xy + local * ring.destination.zw,
        0.0,
        1.0);
    output.pixel = local * ring.shape.zw;
    return output;
}

float roundedRectDistance(float2 point, float2 origin, float2 size, float radius)
{
    float2 halfSize = max(size * 0.5, float2(0.5));
    float fittedRadius = clamp(radius, 0.0, min(halfSize.x, halfSize.y));
    float2 distance = abs(point - origin - halfSize) - halfSize + fittedRadius;
    return min(max(distance.x, distance.y), 0.0)
        + length(max(distance, float2(0.0)))
        - fittedRadius;
}

float filledCoverage(float distance)
{
    float antialiasWidth = max(fwidth(distance), 0.75);
    return 1.0 - smoothstep(-antialiasWidth, antialiasWidth, distance);
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    float outer = filledCoverage(roundedRectDistance(
        input.pixel,
        float2(0.0),
        ring.shape.zw,
        ring.shape.x));
    float inner = filledCoverage(roundedRectDistance(
        input.pixel,
        ring.innerRect.xy,
        ring.innerRect.zw,
        ring.shape.y));
    float coverage = outer * (1.0 - inner);
    if (coverage <= 0.0)
    {
        discard;
    }

    float alpha = ring.color.a * coverage;
    return float4(ring.color.rgb * alpha, alpha);
}
