Texture2D<float4> sourceTexture : register(t0);
SamplerState sourceSampler : register(s0);

struct FragmentInput
{
    [[vk::location(0)]] float2 texCoord : TEXCOORD0;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    // The verified WE pixel shader is exactly frc(v_TexCoord) followed by a
    // direct sample of the owner-local effect target.
    return sourceTexture.Sample(sourceSampler, frac(input.texCoord));
}
