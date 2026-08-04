// Folder-preview composite: rotated quads sampling pre-rendered child
// thumbnails, plus solid rounded frames and a blurred drop shadow.
//
// The push block starts with the direct descriptor-heap indices of the child
// texture and shared sampler; `PreviewCompositeParams` in
// `vulkan_icon/folder_preview.rs` mirrors this layout exactly.

struct PreviewParams
{
    uint imageIndex;
    uint samplerIndex;
    float radius;
    float mode;
    float4 rect;
    float4 color;
    float2 canvas;
    float angle;
    float inset;
    float blur;
    float opacity;
};

[[vk::push_constant]] ConstantBuffer<PreviewParams> params;

static const float2 kCorners[6] =
{
    float2(0.0, 0.0),
    float2(0.0, 1.0),
    float2(1.0, 1.0),
    float2(0.0, 0.0),
    float2(1.0, 1.0),
    float2(1.0, 0.0),
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : UV;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 uv = kCorners[vertexIndex];
    float2 center = params.rect.xy + params.rect.zw * 0.5;
    float2 local = (uv - float2(0.5)) * params.rect.zw;
    float cosine = cos(params.angle);
    float sine = sin(params.angle);
    float2 pixel = center + float2(
        local.x * cosine - local.y * sine,
        local.x * sine + local.y * cosine);

    VertexOutput output;
    // Vulkan clip space is y-down, matching the top-left framebuffer origin.
    output.position = float4(pixel / params.canvas * 2.0 - 1.0, 0.0, 1.0);
    output.uv = uv;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    Texture2D<float4> source = DescriptorHandle<Texture2D<float4>>(params.imageIndex);
    SamplerState sourceSampler = DescriptorHandle<SamplerState>(params.samplerIndex);
    if (params.mode > 1.5)
    {
        float2 contentSize = max(params.rect.zw - float2(params.inset * 2.0), float2(1.0));
        float2 sourceUv = (input.uv * params.rect.zw - float2(params.inset)) / contentSize;
        float width;
        float height;
        source.GetDimensions(width, height);
        float2 dimensions = float2(width, height);
        float alpha = 0.0;
        for (int y = -2; y <= 2; y += 1)
        {
            for (int x = -2; x <= 2; x += 1)
            {
                float2 offset = float2(float(x), float(y)) * params.blur / dimensions;
                float2 sampleUv = sourceUv + offset;
                if (all(sampleUv >= float2(0.0)) && all(sampleUv <= float2(1.0)))
                {
                    alpha += source.SampleLevel(sourceSampler, sampleUv, 0.0).a;
                }
            }
        }
        alpha = alpha / 25.0 * params.opacity;
        return float4(params.color.rgb * alpha, alpha);
    }
    if (params.mode > 0.5)
    {
        float2 halfSize = params.rect.zw * 0.5;
        float2 point = abs((input.uv - float2(0.5)) * params.rect.zw);
        float radius = min(params.radius, min(halfSize.x, halfSize.y));
        float2 delta = max(point - (halfSize - float2(radius)), float2(0.0));
        if (length(delta) > radius)
        {
            discard;
        }
        // The frame draw composites a solid rounded quad; no source sampling.
        return params.color;
    }
    return source.Sample(sourceSampler, input.uv) * params.color;
}
