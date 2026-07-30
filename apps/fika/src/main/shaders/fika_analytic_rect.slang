// Instanced analytic rounded rectangle with clip and optional outline.
// All coordinates are Vulkan NDC with a positive-height, y-down viewport.

struct VertexInput
{
    [[vk::location(0)]] float4 rect : RECT;
    [[vk::location(1)]] float4 clip : CLIP;
    [[vk::location(2)]] float4 color : COLOR;
    [[vk::location(3)]] float4 style : STYLE;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 pixel : PIXEL;
    [[vk::location(1)]] nointerpolation float4 rect : RECT;
    [[vk::location(2)]] nointerpolation float4 clip : CLIP;
    [[vk::location(3)]] nointerpolation float4 color : COLOR;
    [[vk::location(4)]] nointerpolation float4 style : STYLE;
};

static const float2 kCorners[6] =
{
    float2(0.0, 0.0),
    float2(0.0, 1.0),
    float2(1.0, 1.0),
    float2(0.0, 0.0),
    float2(1.0, 1.0),
    float2(1.0, 0.0),
};

[shader("vertex")]
VertexOutput vertexMain(VertexInput input, uint vertexIndex : SV_VulkanVertexID)
{
    float2 corner = kCorners[vertexIndex];
    float2 pixel = float2(
        lerp(input.rect.x, input.rect.z, corner.x),
        lerp(input.rect.y, input.rect.w, corner.y));

    VertexOutput output;
    output.position = float4(pixel, 0.0, 1.0);
    output.pixel = pixel;
    output.rect = input.rect;
    output.clip = input.clip;
    output.color = input.color;
    output.style = input.style;
    return output;
}

// Signed distance to a rounded rectangle. `radius` may be anisotropic because
// NDC units differ per axis; the distance is normalized by the radius so one
// smoothstep covers both axes.
float roundedDistance(float2 local, float2 halfSize, float2 radius)
{
    if (radius.x <= 0.0 || radius.y <= 0.0)
    {
        float2 edge = abs(local) - halfSize;
        return max(edge.x, edge.y);
    }
    float2 corner = max(halfSize - radius, float2(0.0));
    float2 scaled = (abs(local) - corner) / max(radius, float2(1.0e-6));
    float2 outside = max(scaled, float2(0.0));
    return length(outside) + min(max(scaled.x, scaled.y), 0.0) - 1.0;
}

float outsideFraction(float distance)
{
    float antialias = max(fwidth(distance), 1.0e-6);
    return smoothstep(-antialias, antialias, distance);
}

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    float2 pixel = input.pixel;
    if (pixel.x < input.clip.x || pixel.x > input.clip.z
        || pixel.y < input.clip.y || pixel.y > input.clip.w)
    {
        discard;
    }
    float2 center = float2(
        (input.rect.x + input.rect.z) * 0.5,
        (input.rect.y + input.rect.w) * 0.5);
    float2 halfSize = float2(
        (input.rect.z - input.rect.x) * 0.5,
        (input.rect.w - input.rect.y) * 0.5);
    float2 local = pixel - center;
    float2 radius = input.style.xy;
    float2 stroke = input.style.zw;

    float outer = outsideFraction(roundedDistance(local, halfSize, radius));
    float coverage;
    if (stroke.x > 0.0 && stroke.y > 0.0)
    {
        float2 innerHalf = max(halfSize - stroke, float2(0.0));
        float2 innerRadius = max(radius - stroke, float2(0.0));
        float inner = outsideFraction(roundedDistance(local, innerHalf, innerRadius));
        coverage = max(inner - outer, 0.0);
    }
    else
    {
        coverage = 1.0 - outer;
    }
    return float4(input.color.rgb, input.color.a * coverage);
}
