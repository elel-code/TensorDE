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
    [[vk::location(0)]] float2 framebufferCoord : TEXCOORD0;
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
    output.position = float4(input.texCoord * 2.0 - 1.0, 0.0, 1.0);
    // The Vulkan scene projection already folds in WE's explicit D3D framebuffer-Y
    // conjugation. Preserve the authored homogeneous divide, including its sign.
    output.framebufferCoord = projected.xy / projected.w * 0.5 + 0.5;
    return output;
}
