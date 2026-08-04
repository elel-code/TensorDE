#define vec2 float2
#define vec3 float3
#define vec4 float4
#define ivec2 int2
#define ivec3 int3
#define ivec4 int4
#define uvec2 uint2
#define uvec3 uint3
#define uvec4 uint4
#define bvec2 bool2
#define bvec3 bool3
#define bvec4 bool4
#define mat2 float2x2
#define mat3 float3x3
#define mat4 float4x4
#define fract frac
#define mix lerp
#define inversesqrt rsqrt
#define roundEven round
#define texture2D(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture3D(S, UVW) S ## _texture.Sample(S ## _sampler, UVW)
#define texture(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture2DLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define textureLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define texelFetch(S, COORD, LOD) S ## _texture.Load(int3(COORD, LOD))
#define textureSize(S, LOD) tensorWallpaperTextureSize_ ## S(LOD)
#define greaterThan(A, B) ((A) > (B))
#define greaterThanEqual(A, B) ((A) >= (B))
#define lessThan(A, B) ((A) < (B))
#define lessThanEqual(A, B) ((A) <= (B))
#define equal(A, B) ((A) == (B))
#define notEqual(A, B) ((A) != (B))
#define mod(A, B) ((A) - (B) * floor((A) / (B)))

Texture2D<float4> g_Texture0_texture : register(t0);
SamplerState g_Texture0_sampler : register(s0);
int2 tensorWallpaperTextureSize_g_Texture0(uint mip)
{
    uint width;
    uint height;
    uint levels;
    g_Texture0_texture.GetDimensions(mip, width, height, levels);
    return int2(width, height);
}
struct ParticleMaterial
{
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
};
cbuffer ParticleMaterial_Buffer : register(b3)
{
    ParticleMaterial g_Material;
}

static vec2 v_TexCoord;
static float v_VertexAlpha;
static vec3 v_ParticleColor;
static float v_TextureDecodeMode;
static vec4 o_Color;
struct TensorWallpaperFragmentInput
{
    [[vk::location(0)]] vec2 v_TexCoord : TEXCOORD0;
    [[vk::location(1)]] float v_VertexAlpha : TEXCOORD1;
    [[vk::location(2)]] vec3 v_ParticleColor : TEXCOORD2;
    nointerpolation [[vk::location(3)]] float v_TextureDecodeMode : TEXCOORD3;
};
struct TensorWallpaperFragmentOutput
{
    [[vk::location(0)]] vec4 o_Color : SV_Target0;
};

[[shader("fragment")]]
TensorWallpaperFragmentOutput main(TensorWallpaperFragmentInput input)
{
    v_TexCoord = input.v_TexCoord;
    v_VertexAlpha = input.v_VertexAlpha;
    v_ParticleColor = input.v_ParticleColor;
    v_TextureDecodeMode = input.v_TextureDecodeMode;
    TensorWallpaperFragmentOutput output;

    if (v_VertexAlpha <= 0.0) {
        discard;
    }
    vec4 texel = texture(g_Texture0, v_TexCoord);
    if (v_TextureDecodeMode > 0.5) {
        texel = vec4(texel.rrr, texel.g);
    }
    if (texel.a <= 0.0) {
        discard;
    }
    vec4 color = texel * g_Material.g_Color4;
    color.rgb *= v_ParticleColor;
    color.a *= v_VertexAlpha;
    o_Color = color;
    output.o_Color = o_Color;
    return output;
}
