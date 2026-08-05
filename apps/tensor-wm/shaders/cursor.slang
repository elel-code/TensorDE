// Analytic fallback cursor: an arrow evaluated in the fragment shader.

struct CursorData
{
    float4 destination;
};

[[vk::push_constant]] ConstantBuffer<CursorData> cursor;

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
    [[vk::location(0)]] float2 local : TEXCOORD0;
};

[shader("vertex")]
VertexOutput vertexMain(uint vertexIndex : SV_VulkanVertexID)
{
    float2 local = kPositions[vertexIndex];
    float2 position = cursor.destination.xy + local * cursor.destination.zw;

    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.local = local;
    return output;
}

bool containsPoint(float2 point, float2 vertices[7])
{
    bool inside = false;
    int previous = 6;
    for (int index = 0; index < 7; previous = index++)
    {
        float2 left = vertices[index];
        float2 right = vertices[previous];
        bool crosses = (left.y > point.y) != (right.y > point.y);
        if (crosses)
        {
            float x = (right.x - left.x) * (point.y - left.y) / (right.y - left.y) + left.x;
            if (point.x < x)
            {
                inside = !inside;
            }
        }
    }
    return inside;
}

static const float2 kOutline[7] =
{
    float2(0.04, 0.00),
    float2(0.04, 0.83),
    float2(0.25, 0.61),
    float2(0.45, 0.98),
    float2(0.63, 0.89),
    float2(0.43, 0.52),
    float2(0.84, 0.52),
};

static const float2 kFill[7] =
{
    float2(0.13, 0.15),
    float2(0.13, 0.62),
    float2(0.27, 0.48),
    float2(0.46, 0.84),
    float2(0.50, 0.82),
    float2(0.31, 0.45),
    float2(0.62, 0.45),
};

[shader("fragment")]
float4 fragmentMain(VertexOutput input) : SV_Target0
{
    if (!containsPoint(input.local, kOutline))
    {
        discard;
    }
    return containsPoint(input.local, kFill)
        ? float4(1.0, 1.0, 1.0, 1.0)
        : float4(0.0, 0.0, 0.0, 1.0);
}
