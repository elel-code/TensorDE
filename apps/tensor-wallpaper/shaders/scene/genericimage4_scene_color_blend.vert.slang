struct SceneDrawTransformData
{
    float4 modelViewProjectionMatrix[4];
};

cbuffer SceneDrawTransform : register(b2)
{
    SceneDrawTransformData drawTransform;
}

struct VertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 texCoord : TEXCOORD0;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
    [[vk::location(1)]] float3 screenPosition : TEXCOORD1;
};

[[shader("vertex")]]
VertexOutput main(VertexInput input)
{
    float4 localPosition = float4(input.position, 0.0, 1.0);
    float4 projected = float4(
        dot(drawTransform.modelViewProjectionMatrix[0], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[1], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[2], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[3], localPosition));
    VertexOutput output;
    output.position = projected;
    output.texCoord = input.texCoord;
    // Scene projection has already normalized D3D clip Y for Vulkan's positive-height viewport.
    // Preserve that normalized Y here so the scene snapshot is sampled at the rasterized pixel.
    output.screenPosition = float3(projected.x, projected.y, projected.w);
    return output;
}
