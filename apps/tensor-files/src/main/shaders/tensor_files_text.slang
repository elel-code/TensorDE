// R8 text atlas sampling with per-vertex color.
//
// The atlas image and sampler are selected through direct descriptor-heap
// indices pushed per frame; see `VulkanTextRenderer::draw`.

struct TextPush
{
    uint imageIndex;
    uint samplerIndex;
};

[[vk::push_constant]] ConstantBuffer<TextPush> push;

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 uv : UV;
    [[vk::location(2)]] float4 color : COLOR;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : UV;
    [[vk::location(1)]] float4 color : COLOR;
};

[shader("vertex")]
VertexOutput vertexMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    Texture2D<float4> atlas = DescriptorHandle<Texture2D<float4>>(push.imageIndex);
    SamplerState atlasSampler = DescriptorHandle<SamplerState>(push.samplerIndex);
    float mask = atlas.Sample(atlasSampler, input.uv).r;
    return float4(input.color.rgb, input.color.a * mask);
}
