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
    [[vk::location(2)]] float opacity : TEXCOORD1;
    [[vk::location(3)]] uint4 blendIndices : BLENDINDICES;
    [[vk::location(4)]] float4 blendWeights : BLENDWEIGHT;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
    [[vk::location(1)]] float vertexAlpha : TEXCOORD1;
};

[[shader("vertex")]]
VertexOutput main(VertexInput input)
{
    float4 localPosition = float4(input.position, 0.0, 1.0);
    VertexOutput output;
    output.position = float4(
        dot(drawTransform.modelViewProjectionMatrix[0], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[1], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[2], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[3], localPosition));
    output.texCoord = input.texCoord;
    output.vertexAlpha = input.opacity;
    return output;
}
