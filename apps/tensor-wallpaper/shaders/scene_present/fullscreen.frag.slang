Texture2D<float4> sceneColor : register(t0);
SamplerState sceneColorSampler : register(s0);

struct FragmentInput
{
    [[vk::location(0)]] float2 uv : TEXCOORD0;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float3 color = sceneColor.Sample(sceneColorSampler, input.uv).rgb;
    return float4(color, 1.0);
}
