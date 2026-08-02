struct VertexInput
{
    uint vertexId : SV_VertexID;
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
    float2 texCoord = input.vertexId == 0 ? float2(0.0, 0.0)
        : input.vertexId == 1 ? float2(1.0, 0.0)
        : input.vertexId == 2 ? float2(0.0, 1.0)
        : input.vertexId == 3 ? float2(0.0, 1.0)
        : input.vertexId == 4 ? float2(1.0, 0.0)
                              : float2(1.0, 1.0);
    VertexOutput output;
    output.position = float4(texCoord * 2.0 - 1.0, 0.0, 1.0);
    output.texCoord = texCoord;
    output.vertexAlpha = 1.0;
    return output;
}
