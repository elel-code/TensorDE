//! Typed WE tint shader variants.

use super::effect_program::effect_combo_value_for_key;

pub(crate) fn tint_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 1, "tint requires authored source slot 0");
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 30);
    let blend_expression = match blend_mode {
        0 => "mix(albedo.rgb, u_Effect.g_AlphaColor.yzw, alpha)",
        30 => {
            "mix(albedo.rgb, vec3(max(albedo.r, max(albedo.g, albedo.b))) * u_Effect.g_AlphaColor.yzw, alpha)"
        }
        _ => panic!("tint shader {key:?} has no typed blend-mode contract"),
    };
    let alpha_expression = if blend_mode == 0 {
        "    albedo.a = 1.0;\n"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform TintUniform {{
    vec4 g_AlphaColor;
}} u_Effect;
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float alpha = clamp(u_Effect.g_AlphaColor.x, 0.0, 1.0);
    albedo.rgb = {blend_expression};
{alpha_expression}    o_Color = albedo;
}}
"#
    )
}

pub(crate) fn tint_masked_sources(key: &str) -> (String, String) {
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 30);
    let blend_expression = match blend_mode {
        0 => "material.alphaColor.yzw",
        30 => "float3(max(sample.x, max(sample.y, sample.z))) * material.alphaColor.yzw",
        _ => panic!("masked tint shader {key:?} has no typed blend-mode contract"),
    };
    let alpha_expression = if blend_mode == 0 { "1.0" } else { "sample.w" };
    (
        format!(
            r#"{material}
struct TintFullscreenVertexInput
{{
    uint vertexId : SV_VertexID;
}};

struct TintVertexOutput
{{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
}};

[[shader("vertex")]]
TintVertexOutput main(TintFullscreenVertexInput input)
{{
    float2 position = input.vertexId == 0
        ? float2(-1.0, -1.0)
        : input.vertexId == 1 ? float2(3.0, -1.0) : float2(-1.0, 3.0);
    float2 texCoord = position * 0.5 + 0.5;
    TintVertexOutput output;
    output.position = float4(position, 0.0, 1.0);
    output.texCoord = float4(
        texCoord,
        texCoord * material.texture1Resolution.zw / material.texture1Resolution.xy);
    return output;
}}
"#,
            material = tint_masked_material_source(),
        ),
        format!(
            r#"Texture2D<float4> sourceTexture : register(t0);
Texture2D<float4> maskTexture : register(t1);
SamplerState sourceSampler : register(s0);
SamplerState maskSampler : register(s1);
{material}
struct TintFragmentInput
{{
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
}};

[[shader("fragment")]]
float4 main(TintFragmentInput input) : SV_Target0
{{
    float4 sample = sourceTexture.Sample(sourceSampler, input.texCoord.xy);
    float mask = saturate(material.alphaColor.x)
        * maskTexture.Sample(maskSampler, input.texCoord.zw).x;
    float3 blended = {blend_expression};
    return float4(lerp(sample.xyz, blended, mask), {alpha_expression});
}}
"#,
            material = tint_masked_material_source(),
        ),
    )
}

pub(crate) fn tint_masked_object_mesh_vertex_source() -> String {
    format!(
        r#"struct SceneDrawTransformData
{{
    float4 modelViewProjectionMatrix[4];
}};

cbuffer SceneDrawTransform : register(b2)
{{
    SceneDrawTransformData drawTransform;
}}

{material}
struct TintObjectVertexInput
{{
    [[vk::location(0)]] float2 position : POSITION;
    [[vk::location(1)]] float2 texCoord : TEXCOORD0;
}};

struct TintVertexOutput
{{
    float4 position : SV_Position;
    [[vk::location(0)]] float4 texCoord : TEXCOORD0;
}};

[[shader("vertex")]]
TintVertexOutput main(TintObjectVertexInput input)
{{
    float4 localPosition = float4(input.position, 0.0, 1.0);
    TintVertexOutput output;
    output.position = float4(
        dot(drawTransform.modelViewProjectionMatrix[0], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[1], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[2], localPosition),
        dot(drawTransform.modelViewProjectionMatrix[3], localPosition));
    output.texCoord = float4(
        input.texCoord,
        input.texCoord * material.texture1Resolution.zw / material.texture1Resolution.xy);
    return output;
}}
"#,
        material = tint_masked_material_source(),
    )
}

fn tint_masked_material_source() -> &'static str {
    r#"struct TintMaterialData
{
    float4 alphaColor;
    float4 texture1Resolution;
};

cbuffer TintMaterial : register(b3)
{
    TintMaterialData material;
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tint_and_normal_replace_are_distinct() {
        let tint = tint_fragment_source("effects/tint__SLOTS_1", 1);
        let normal = tint_fragment_source("effects/tint__SLOTS_1__BLENDMODE_0", 1);
        assert!(tint.contains("max(albedo.r"));
        assert!(!tint.contains("albedo.a = 1.0"));
        assert!(normal.contains("albedo.a = 1.0"));
    }
}
