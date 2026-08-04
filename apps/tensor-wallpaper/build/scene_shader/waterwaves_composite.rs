pub(crate) fn waterwaves_uv_field_sources() -> (String, String) {
    (
        waterwaves_uv_vertex_source(),
        waterwaves_uv_field_fragment_source(),
    )
}

pub(crate) fn image_waterwaves_composite_sources() -> (String, String) {
    (
        super::super::scene_mesh_vertex_source(),
        image_waterwaves_composite_fragment(false),
    )
}

pub(crate) fn image_waterwaves_multiply_composite_sources() -> (String, String) {
    (
        super::super::scene_mesh_vertex_source(),
        image_waterwaves_composite_fragment(true),
    )
}

pub(crate) fn puppet_waterwaves_composite_sources() -> (String, String) {
    (
        super::puppet_effect_composite_vertex(),
        puppet_waterwaves_composite_fragment(),
    )
}

fn image_waterwaves_composite_fragment(premultiply: bool) -> String {
    let premultiply = if premultiply {
        "    color.rgb *= color.a;\n"
    } else {
        ""
    };
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform ImageWaterWavesCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
    vec4 g_EffectAtlas;
} g_Material;
void main() {
    vec2 atlas_size = vec2(textureSize(g_Texture1, 0));
    vec2 atlas_min = g_Material.g_EffectAtlas.zw + 0.5 / atlas_size;
    vec2 atlas_max = g_Material.g_EffectAtlas.zw
        + g_Material.g_EffectAtlas.xy - 0.5 / atlas_size;
    vec2 atlas_uv = mix(atlas_min, atlas_max, clamp(v_TexCoord, 0.0, 1.0));
    vec2 source_uv = clamp(texture(g_Texture1, atlas_uv).rg, vec2(0.001), vec2(0.999));
    vec4 source_color = texture(g_Texture0, source_uv);
    const float alpha_noise = 4.0 / 255.0;
    source_color.a = max(source_color.a - alpha_noise, 0.0) / (1.0 - alpha_noise);
    if (source_color.a == 0.0) {
        discard;
    }
    vec4 color = source_color * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}

fn puppet_waterwaves_composite_fragment() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_EffectTexCoord;
layout(location = 1) in float v_BoneAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform PuppetWaterWavesCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
    vec4 g_EffectAtlas;
} g_Material;
void main() {
    vec2 atlas_size = vec2(textureSize(g_Texture1, 0));
    vec2 atlas_min = g_Material.g_EffectAtlas.zw + 0.5 / atlas_size;
    vec2 atlas_max = g_Material.g_EffectAtlas.zw
        + g_Material.g_EffectAtlas.xy - 0.5 / atlas_size;
    vec2 atlas_uv = mix(atlas_min, atlas_max, clamp(v_EffectTexCoord, 0.0, 1.0));
    vec2 source_uv = clamp(texture(g_Texture1, atlas_uv).rg, vec2(0.001), vec2(0.999));
    vec4 color = texture(g_Texture0, source_uv);
    const float alpha_noise = 4.0 / 255.0;
    color.a = max(color.a - alpha_noise, 0.0) / (1.0 - alpha_noise);
    if (color.a == 0.0) {
        discard;
    }
    color *= g_Material.g_Color4;
    color.a *= v_BoneAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn waterwaves_uv_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform WaterWavesDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec4 v_ObjectUvToScreenUv;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    float layer = u_Draw.g_ScreenUvToObjectUvRow0.w;
    vec2 atlas_grid = max(vec2(
        u_Draw.g_ScreenUvToObjectUvRow1.w,
        u_Draw.g_ObjectUvToScreenUvRow0.w), vec2(1.0));
    vec2 atlas_tile = vec2(mod(layer, atlas_grid.x), floor(layer / atlas_grid.x));
    vec2 atlas_uv = (atlas_tile + uv) / atlas_grid;
    v_TexCoord = uv;
    v_ObjectTexCoord = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(uv, 1.0)));
    v_ObjectUvToScreenUv = vec4(
        u_Draw.g_ObjectUvToScreenUvRow0.xy,
        u_Draw.g_ObjectUvToScreenUvRow1.xy);
    gl_Position = vec4(atlas_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterwaves_uv_field_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec2 o_Uv;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 3) uniform WaterWavesUvFieldUniform {
    vec4 g_Chain;
    vec4 g_Stage[36];
} u_Effect;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 6) uniform sampler2D g_Texture6;
layout(set = 0, binding = 7) uniform sampler2D g_Texture7;
layout(set = 0, binding = 8) uniform sampler2D g_Texture8;
layout(set = 0, binding = 9) uniform sampler2D g_Texture9;
layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    if (exponent == 1.0) return wave;
    if (exponent == 2.0) return wave * abs(wave);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}
float stageMask(int stage, vec2 uv) {
    if (stage == 0) return texture(g_Texture1, uv).r;
    if (stage == 1) return texture(g_Texture2, uv).r;
    if (stage == 2) return texture(g_Texture3, uv).r;
    if (stage == 3) return texture(g_Texture4, uv).r;
    if (stage == 4) return texture(g_Texture5, uv).r;
    if (stage == 5) return texture(g_Texture6, uv).r;
    if (stage == 6) return texture(g_Texture7, uv).r;
    if (stage == 7) return texture(g_Texture8, uv).r;
    return texture(g_Texture9, uv).r;
}
vec2 stageOffset(int stage, vec2 motion_uv) {
    int base = stage * 4;
    vec4 phase_scale_strength2_mask = u_Effect.g_Stage[base];
    vec4 direction_phase2_scale2 = u_Effect.g_Stage[base + 1];
    vec4 direction2_exponents = u_Effect.g_Stage[base + 2];
    vec4 mask_resolution = u_Effect.g_Stage[base + 3];
    if (phase_scale_strength2_mask.z <= 0.0) return vec2(0.0);
    float mask = 1.0;
    if (phase_scale_strength2_mask.w > 0.5) {
        vec2 mask_uv = motion_uv * mask_resolution.zw / mask_resolution.xy;
        mask = stageMask(stage, mask_uv);
    }
    vec2 direction = direction_phase2_scale2.xy;
    float distance0 = phase_scale_strength2_mask.x
        + dot(motion_uv, direction) * phase_scale_strength2_mask.y;
    vec2 offset_direction = vec2(direction.y, -direction.x);
    float displacement = shapedSine(distance0, direction2_exponents.z);
    if (direction_phase2_scale2.w > 0.0) {
        vec2 direction2 = direction2_exponents.xy;
        float distance1 = direction_phase2_scale2.z
            + dot(motion_uv, direction2) * direction_phase2_scale2.w;
        displacement *= shapedSine(distance1, direction2_exponents.w);
    }
    vec2 object_uv_offset = displacement * offset_direction
        * phase_scale_strength2_mask.z * mask;
    return vec2(
        dot(v_ObjectUvToScreenUv.xy, object_uv_offset),
        dot(v_ObjectUvToScreenUv.zw, object_uv_offset));
}
void main() {
    int stage_count = clamp(int(u_Effect.g_Chain.x + 0.5), 0, 9);
    vec2 source_uv = v_TexCoord;
    for (int stage = 8; stage >= 0; --stage) {
        if (stage < stage_count) {
            source_uv += stageOffset(stage, source_uv);
        }
    }
    o_Uv = source_uv;
}
"#
    .to_owned()
}
