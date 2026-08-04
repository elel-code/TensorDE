struct SceneDrawTransformData
{
    float4 modelViewProjectionMatrix[4];
};

cbuffer SceneDrawTransform : register(b2)
{
    SceneDrawTransformData drawTransform;
}

struct PuppetBonePalette
{
    float4 row0;
    float4 row1;
    float4 row2;
    float4 row3;
    float4 alpha;
};

StructuredBuffer<PuppetBonePalette> puppetBones : register(t4);

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
    [[vk::location(0)]] float2 effectTexCoord : TEXCOORD0;
    [[vk::location(1)]] float boneAlpha : TEXCOORD1;
    [[vk::location(2)]] float3 clippingScreenPosition : TEXCOORD2;
};

float4 projectPosition(float4 position)
{
    return float4(
        dot(drawTransform.modelViewProjectionMatrix[0], position),
        dot(drawTransform.modelViewProjectionMatrix[1], position),
        dot(drawTransform.modelViewProjectionMatrix[2], position),
        dot(drawTransform.modelViewProjectionMatrix[3], position));
}

[[shader("vertex")]]
VertexOutput main(VertexInput input)
{
    float4 rawPosition = float4(input.position, 0.0, 1.0);
    float4 skinnedPosition = float4(0.0, 0.0, 0.0, 0.0);
    float skinnedAlpha = 0.0;
    float totalWeight = 0.0;
    for (uint slot = 0u; slot < 4u; slot++)
    {
        float weight = input.blendWeights[slot];
        if (weight <= 0.0000001)
        {
            continue;
        }
        PuppetBonePalette bone = puppetBones[input.blendIndices[slot]];
        skinnedPosition += float4(
            dot(bone.row0, rawPosition),
            dot(bone.row1, rawPosition),
            dot(bone.row2, rawPosition),
            dot(bone.row3, rawPosition)) * weight;
        skinnedAlpha += bone.alpha.x * weight;
        totalWeight += weight;
    }
    float4 localPosition = rawPosition;
    float boneAlpha = 1.0;
    if (totalWeight > 0.0000001)
    {
        localPosition = skinnedPosition / totalWeight;
        boneAlpha = skinnedAlpha / totalWeight;
    }
    VertexOutput output;
    float4 projectedPosition = projectPosition(localPosition);
    output.position = projectedPosition;
    output.effectTexCoord = input.texCoord;
    output.boneAlpha = boneAlpha;
    // The scene runtime uses a positive-height Vulkan viewport, so NDC Y
    // already maps directly to the R8 clipping target's framebuffer UV.
    output.clippingScreenPosition = projectedPosition.xyw;
    return output;
}
