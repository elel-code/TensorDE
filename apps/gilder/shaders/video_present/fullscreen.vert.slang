struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
    [[vk::location(1)]] nointerpolation uint layer : TEXCOORD1;
};

[[shader("vertex")]]
VertexOutput main(uint vertexIndex : SV_VertexID, uint instanceIndex : SV_InstanceID)
{
    const float2 positions[3] = {
        float2(-1.0, -1.0),
        float2(3.0, -1.0),
        float2(-1.0, 3.0),
    };
    const float2 coordinates[3] = {
        float2(0.0, 0.0),
        float2(2.0, 0.0),
        float2(0.0, 2.0),
    };
    VertexOutput output;
    output.position = float4(positions[vertexIndex], 0.0, 1.0);
    output.uv = coordinates[vertexIndex];
    output.layer = instanceIndex;
    return output;
}
