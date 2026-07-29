// Client-surface sampling with rounded-corner coverage.
//
// The image descriptor is selected at draw time through Tensor's
// descriptor-heap binding mapping; `descriptorIndex` rides in the same
// 64-byte push record consumed by that mapping.

struct DrawData
{
    uint descriptorIndex;
    uint cornerRadius;
    float opacity;
    float padding;
    float4 destination;
    float4 uvOriginAxisX;
    float4 uvAxisYSurfaceSize;
};

[[vk::push_constant]] ConstantBuffer<DrawData> draw;

[[vk::binding(0, 0)]] Sampler2D clientImage;

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
    [[vk::location(1)]] float2 local : TEXCOORD1;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 local = kPositions[vertexIndex];
    // Tensor converts its top-left physical surface coordinates to Vulkan NDC
    // before recording the draw. Keep this shader free of a second Y flip.
    float2 position = draw.destination.xy + local * draw.destination.zw;

    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.local = local;
    output.texCoord = draw.uvOriginAxisX.xy
        + local.x * draw.uvOriginAxisX.zw
        + local.y * draw.uvAxisYSurfaceSize.xy;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    float4 color = clientImage.Sample(input.texCoord);
    float coverage = draw.opacity;
    float2 surfaceSize = draw.uvAxisYSurfaceSize.zw;
    float radius = min(float(draw.cornerRadius), min(surfaceSize.x, surfaceSize.y) * 0.5);
    if (radius > 0.0)
    {
        float2 distanceToEdge = min(input.local, 1.0 - input.local) * surfaceSize;
        float2 cornerDistance = max(radius - distanceToEdge, float2(0.0));
        float outside = length(cornerDistance);
        float edge = 1.0 - smoothstep(max(radius - 1.0, 0.0), radius, outside);
        coverage *= edge;
    }
    return color * coverage;
}
