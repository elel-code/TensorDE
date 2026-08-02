Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
    [[vk::location(1)]] float vertexAlpha : TEXCOORD1;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float4 color = sourceTexture.Sample(sourceSampler, input.texCoord);
    color.a *= input.vertexAlpha;
    return color;
}
