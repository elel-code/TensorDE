struct SkewMaterialData
{
    float4 topBottomLeftRight;
    float4 unused0;
    float4 unused1;
    float4 unused2;
};

cbuffer SkewMaterial : register(b3)
{
    SkewMaterialData material;
}

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
    float2 targetUv = position * 0.5 + 0.5;
    float4 edges = material.topBottomLeftRight;

    // WE evaluates the four edge selectors at the authored quad vertices, then
    // raster interpolation produces this affine coordinate field. Evaluate the
    // affine extension at the oversized triangle vertices so its interior is
    // exactly the same field without introducing a diagonal interpolation seam.
    float2 sampleUv = targetUv;
    sampleUv.x -= lerp(
        edges.x,
        edges.y,
        targetUv.y);
    sampleUv.y += lerp(
        edges.z,
        edges.w,
        targetUv.x);

    VertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord = sampleUv;
    return output;
}
