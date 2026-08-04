struct SceneDrawTransformData
{
    float4 modelViewProjectionMatrix[4];
};

cbuffer SceneDrawTransform : register(b2)
{
    SceneDrawTransformData drawTransform;
}

struct IrisObjectMaterialData
{
    float4 eyeColorBackground;
    float4 timeSpeedRoughNoise;
    float4 scalePhaseMask;
    float4 texture1Resolution;
};

cbuffer IrisObjectMaterial : register(b3)
{
    IrisObjectMaterialData material;
}

struct IrisObjectVertexInput
{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 texCoord : TEXCOORD0;
};

struct IrisObjectVertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
    [[vk::location(1)]] float2 irisOffset : TEXCOORD1;
};

[[shader("vertex")]]
IrisObjectVertexOutput main(IrisObjectVertexInput input)
{
    IrisObjectVertexOutput output;
    output.texCoord = input.texCoord.xyxy;
    if (material.scalePhaseMask.w > 0.5
        && material.texture1Resolution.x > 0.0
        && material.texture1Resolution.y > 0.0)
    {
        output.texCoord.zw = float2(
            input.texCoord.x * material.texture1Resolution.z
                / material.texture1Resolution.x,
            input.texCoord.y * material.texture1Resolution.w
                / material.texture1Resolution.y);
    }

    float time = material.timeSpeedRoughNoise.x * material.timeSpeedRoughNoise.y
        + material.scalePhaseMask.z;
    float lowDt = floor(time);
    float2 motion2 = sin(1.9 * (lowDt + float2(0.0, 1.0)));
    float4 motion4 = sin(
        2.5 * (lowDt + float4(0.0, 0.0, 1.0, 1.0))
            + float4(1.0, 2.0, 1.0, 2.0));
    float2 moveStart = motion2.xx + motion4.xy;
    float2 moveEnd = motion2.yy + motion4.zw;
    float smoothInput = cos(frac(time) * 3.14159265358979323846) * -0.5 + 0.5;
    float2 offset = lerp(
        moveStart,
        moveEnd,
        smoothstep(
            1.0 - material.timeSpeedRoughNoise.z,
            1.0,
            smoothInput));
    offset.x += sin(time) * material.timeSpeedRoughNoise.w;
    offset.y += cos(time) * material.timeSpeedRoughNoise.w;
    output.irisOffset = offset * material.scalePhaseMask.xy * 0.001;

    float4 localPosition = float4(input.position, 0.0, 1.0);
    output.position = float4(
        dot(drawTransform.modelViewProjectionMatrix[0], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[1], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[2], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[3], localPosition));
    return output;
}
