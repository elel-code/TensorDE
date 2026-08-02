Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);

struct FragmentInput
{
    [[vk::location(0)]] float2 framebufferCoord : TEXCOORD0;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    return sourceTexture.Sample(sourceSampler, input.framebufferCoord);
}
