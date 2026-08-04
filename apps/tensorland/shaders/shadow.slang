// Analytic rounded-rectangle Gaussian shadow.
//
// The mask integration is based on Evan Wallace's CC0 fast rounded rectangle
// shadow technique, also used by Niri. Tensor keeps it in the direct output
// pass: no sampled image, descriptor, or retained intermediate is required.

struct ShadowData
{
    float4 destination;
    float4 color;
    float4 boxRect;
    float4 shape;
};

[[vk::push_constant]] ConstantBuffer<ShadowData> shadow;

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
    [[vk::location(0)]] float2 pixel : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 local = kPositions[vertexIndex];
    VertexOutput output;
    output.position = float4(
        shadow.destination.xy + local * shadow.destination.zw,
        0.0,
        1.0);
    output.pixel = local * shadow.shape.zw;
    return output;
}

float gaussian(float value, float sigma)
{
    static const float pi = 3.141592653589793;
    return exp(-(value * value) / (2.0 * sigma * sigma))
        / (sqrt(2.0 * pi) * sigma);
}

float2 erfApprox(float2 value)
{
    float2 signValue = sign(value);
    float2 absolute = abs(value);
    float2 denominator = 1.0
        + (0.278393 + (0.230389 + 0.078108 * absolute * absolute) * absolute)
            * absolute;
    denominator *= denominator;
    return signValue - signValue / (denominator * denominator);
}

float roundedBoxShadowX(float x, float y, float sigma, float corner, float2 halfSize)
{
    float delta = min(halfSize.y - corner - abs(y), 0.0);
    float curved = halfSize.x - corner
        + sqrt(max(0.0, corner * corner - delta * delta));
    float2 integral = 0.5
        + 0.5 * erfApprox((x + float2(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

float roundedBoxShadow(float2 lower, float2 upper, float2 point, float sigma, float corner)
{
    float2 center = (lower + upper) * 0.5;
    float2 halfSize = (upper - lower) * 0.5;
    point -= center;

    float low = point.y - halfSize.y;
    float high = point.y + halfSize.y;
    float start = clamp(-3.0 * sigma, low, high);
    float end = clamp(3.0 * sigma, low, high);
    float stepSize = (end - start) / 4.0;
    float y = start + stepSize * 0.5;
    float value = 0.0;
    [unroll]
    for (int index = 0; index < 4; ++index)
    {
        value += roundedBoxShadowX(point.x, point.y - y, sigma, corner, halfSize)
            * gaussian(y, sigma) * stepSize;
        y += stepSize;
    }
    return value;
}

float roundedRectDistance(float2 point, float2 origin, float2 size, float radius)
{
    float2 halfSize = max(size * 0.5, float2(0.5));
    float fittedRadius = clamp(radius, 0.0, min(halfSize.x, halfSize.y));
    float2 distance = abs(point - origin - halfSize) - halfSize + fittedRadius;
    return min(max(distance.x, distance.y), 0.0)
        + length(max(distance, float2(0.0))) - fittedRadius;
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    float sigma = shadow.shape.x;
    float coverage;
    if (sigma < 0.1)
    {
        float distance = roundedRectDistance(
            input.pixel,
            shadow.boxRect.xy,
            shadow.boxRect.zw,
            shadow.shape.y);
        float antialiasWidth = max(fwidth(distance), 0.75);
        coverage = 1.0 - smoothstep(-antialiasWidth, antialiasWidth, distance);
    }
    else
    {
        coverage = roundedBoxShadow(
            shadow.boxRect.xy,
            shadow.boxRect.xy + shadow.boxRect.zw,
            input.pixel,
            sigma,
            shadow.shape.y);
    }

    float alpha = shadow.color.a * coverage;
    if (alpha <= 0.0)
    {
        discard;
    }
    return float4(shadow.color.rgb * alpha, alpha);
}
