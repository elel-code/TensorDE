struct VertexInput
{
    uint vertexId : SV_VertexID;
};

struct VertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
};

[[shader("vertex")]]
VertexOutput main(VertexInput input)
{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord = position * 0.5 + 0.5;
    return output;
}
