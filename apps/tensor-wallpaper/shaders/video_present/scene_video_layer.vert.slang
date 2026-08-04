struct SceneVideoPush
{
    float2 extent;
};

[[vk::push_constant]] ConstantBuffer<SceneVideoPush> pushData;

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 uv : TEXCOORD0;
    [[vk::location(2)]] float opacity : COLOR0;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
    [[vk::location(1)]] float opacity : COLOR0;
    [[vk::location(2)]] nointerpolation uint layer : TEXCOORD1;
};

[[shader("vertex")]]
VertexOutput main(VertexInput input, uint instanceIndex : SV_InstanceID)
{
    float2 ndc = input.position / pushData.extent * 2.0 - 1.0;
    VertexOutput output;
    output.position = float4(ndc, 0.0, 1.0);
    output.uv = input.uv;
    output.opacity = input.opacity;
    output.layer = instanceIndex;
    return output;
}
