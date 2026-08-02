SubpassInput<float4> sourceInput : register(t64);

[[shader("fragment")]]
float4 main() : SV_Target0
{
    return sourceInput.SubpassLoad();
}
