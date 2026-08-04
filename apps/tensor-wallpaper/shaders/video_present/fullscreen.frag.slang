Texture2DArray<float4> yPlane : register(t0);
Texture2DArray<float4> uvPlane : register(t1);
SamplerState ySampler : register(s0);
SamplerState uvSampler : register(s1);

struct FragmentInput
{
    [[vk::location(0)]] float2 uv : TEXCOORD0;
    [[vk::location(1)]] nointerpolation uint layer : TEXCOORD1;
};

[[shader("fragment")]]
float4 main(FragmentInput input) : SV_Target0
{
    float3 coordinate = float3(input.uv, float(input.layer));
    float y = yPlane.Sample(ySampler, coordinate).r;
    float2 uv = uvPlane.Sample(uvSampler, coordinate).rg - float2(0.5, 0.5);
    float r = y + 1.402 * uv.y;
    float g = y - 0.344136 * uv.x - 0.714136 * uv.y;
    float b = y + 1.772 * uv.x;
    return float4(r, g, b, 1.0);
}
