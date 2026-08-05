// Managed client-surface color transform. The ordinary client shader remains
// the zero-overhead SDR identity path; this module is selected only when the
// retained per-surface ColorTransformPlan is non-identity.

struct DrawData
{
    uint descriptorIndex;
    uint cornerRadius;
    float opacity;
    uint samplerIndex;
    float4 destination;
    float4 uvOriginAxisX;
    float4 uvAxisYSurfaceSize;
    uint colorModes;
    float sourcePower;
    float targetPower;
    float toneScale;
    float4 gamutRow0;
    float4 gamutRow1;
    float4 gamutRow2;
};

[[vk::push_constant]] ConstantBuffer<DrawData> draw;

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
    float2 position = draw.destination.xy + local * draw.destination.zw;

    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.local = local;
    output.texCoord = draw.uvOriginAxisX.xy
        + local.x * draw.uvOriginAxisX.zw
        + local.y * draw.uvAxisYSurfaceSize.xy;
    return output;
}

float decodePq(float encoded)
{
    const float m1 = 0.1593017578125;
    const float m2 = 78.84375;
    const float c1 = 0.8359375;
    const float c2 = 18.8515625;
    const float c3 = 18.6875;
    float e = pow(saturate(encoded), 1.0 / m2);
    return pow(max(e - c1, 0.0) / max(c2 - c3 * e, 0.000001), 1.0 / m1);
}

float encodePq(float linear)
{
    const float m1 = 0.1593017578125;
    const float m2 = 78.84375;
    const float c1 = 0.8359375;
    const float c2 = 18.8515625;
    const float c3 = 18.6875;
    float e = pow(saturate(linear), m1);
    return pow((c1 + c2 * e) / (1.0 + c3 * e), m2);
}

float decodeHlg(float encoded)
{
    const float a = 0.17883277;
    const float b = 0.28466892;
    const float c = 0.55991073;
    return encoded <= 0.5
        ? encoded * encoded / 3.0
        : (exp((encoded - c) / a) + b) / 12.0;
}

float encodeHlg(float linear)
{
    const float a = 0.17883277;
    const float b = 0.28466892;
    const float c = 0.55991073;
    return linear <= (1.0 / 12.0)
        ? sqrt(max(3.0 * linear, 0.0))
        : a * log(max(12.0 * linear - b, 0.000001)) + c;
}

float decodeTransfer(float encoded, uint mode, float power)
{
    if (mode == 0)
        return encoded;
    if (mode == 1)
        return encoded <= 0.04045
            ? encoded / 12.92
            : pow((encoded + 0.055) / 1.055, 2.4);
    if (mode == 2)
        return pow(max(encoded, 0.0), 2.4);
    if (mode == 3)
        return pow(max(encoded, 0.0), power);
    if (mode == 4)
        return decodePq(encoded);
    return decodeHlg(encoded);
}

float encodeTransfer(float linear, uint mode, float power)
{
    if (mode == 0)
        return linear;
    if (mode == 1)
        return linear <= 0.0031308
            ? linear * 12.92
            : 1.055 * pow(max(linear, 0.0), 1.0 / 2.4) - 0.055;
    if (mode == 2)
        return pow(max(linear, 0.0), 1.0 / 2.4);
    if (mode == 3)
        return pow(max(linear, 0.0), 1.0 / power);
    if (mode == 4)
        return encodePq(linear);
    return encodeHlg(linear);
}

float3 decodeTransfer(float3 encoded, uint mode, float power)
{
    return float3(
        decodeTransfer(encoded.x, mode, power),
        decodeTransfer(encoded.y, mode, power),
        decodeTransfer(encoded.z, mode, power));
}

float3 encodeTransfer(float3 linear, uint mode, float power)
{
    return float3(
        encodeTransfer(linear.x, mode, power),
        encodeTransfer(linear.y, mode, power),
        encodeTransfer(linear.z, mode, power));
}

float3 applyToneMap(float3 linear)
{
    float targetReference = max(draw.gamutRow1.w, 1.0);
    float sourcePeak = max(draw.gamutRow2.w / targetReference, 1.0);
    float targetPeak = max(sourcePeak * draw.toneScale, 0.000001);
    float value = max(linear.x, max(linear.y, linear.z));
    float knee = targetPeak * 0.75;
    if (value <= knee)
        return linear;

    float t = saturate((value - knee) / max(sourcePeak - knee, 0.000001));
    float t2 = t * t;
    float t3 = t2 * t;
    float mapped = (2.0 * t3 - 3.0 * t2 + 1.0) * knee
        + (t3 - 2.0 * t2 + t) * (targetPeak - knee)
        + (-2.0 * t3 + 3.0 * t2) * targetPeak;
    return linear * (mapped / max(value, 0.000001));
}

float4 transformColor(float4 sampled)
{
    uint sourceTransfer = draw.colorModes & 0xf;
    uint targetTransfer = (draw.colorModes >> 4) & 0xf;
    uint alphaMode = (draw.colorModes >> 8) & 0x3;
    float alpha = sampled.a;
    float3 encoded = sampled.rgb;

    if ((draw.colorModes & (1u << 10)) != 0)
        encoded = saturate((encoded - (16.0 / 255.0)) * (255.0 / 219.0));
    if (alphaMode == 0 && alpha > 0.0)
        encoded /= alpha;

    float3 linear = decodeTransfer(encoded, sourceTransfer, draw.sourcePower);
    if (alphaMode == 1 && alpha > 0.0)
        linear /= alpha;
    float targetReference = max(draw.gamutRow1.w, 1.0);
    linear *= sourceTransfer == 4
        ? 10000.0 / targetReference
        : draw.gamutRow0.w / targetReference;

    if ((draw.colorModes & (1u << 11)) != 0)
    {
        linear = float3(
            dot(draw.gamutRow0.xyz, linear),
            dot(draw.gamutRow1.xyz, linear),
            dot(draw.gamutRow2.xyz, linear));
    }
    if ((draw.colorModes & (1u << 12)) != 0)
        linear = applyToneMap(linear);

    float3 targetLinear = targetTransfer == 4
        ? linear * (targetReference / 10000.0)
        : linear;
    float3 target = encodeTransfer(targetLinear, targetTransfer, draw.targetPower);
    return float4(target * alpha, alpha);
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    Texture2D<float4> clientImage = DescriptorHandle<Texture2D<float4>>(draw.descriptorIndex);
    SamplerState clientSampler = DescriptorHandle<SamplerState>(draw.samplerIndex);
    float4 color = transformColor(clientImage.Sample(clientSampler, input.texCoord));
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
