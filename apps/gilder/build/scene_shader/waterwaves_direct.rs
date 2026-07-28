//! Direct typed evaluation of a complete authored WaterWaves chain.
//!
//! Authored sampling reference: `reverse-engineered/shaders/effects/waterwaves.frag`.

pub(crate) fn waterwaves_direct_sources(
    puppet_skinning: bool,
    fullscreen_effect: bool,
    premultiply_output: bool,
    stage_count: Option<usize>,
    static_black_output: bool,
) -> (String, String) {
    let vertex = if fullscreen_effect {
        waterwaves_effect_run_vertex()
    } else if puppet_skinning {
        super::puppet_effect_composite_vertex()
    } else {
        super::super::scene_mesh_vertex_source()
    };
    (
        vertex,
        waterwaves_direct_fragment(
            premultiply_output,
            fullscreen_effect,
            stage_count,
            static_black_output,
        ),
    )
}

fn waterwaves_effect_run_vertex() -> String {
    r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;
    v_VertexAlpha = 1.0;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterwaves_direct_fragment(
    premultiply_output: bool,
    fullscreen_effect: bool,
    stage_count: Option<usize>,
    static_black_output: bool,
) -> String {
    let mask_stage_count = stage_count.unwrap_or(9).min(9);
    let mask_sampler_declarations = (0..mask_stage_count)
        .map(|stage| {
            let slot = stage + 1;
            let binding = if slot == 3 { 35 } else { slot };
            format!("layout(set = 0, binding = {binding}) uniform sampler2D g_Texture{slot};\n")
        })
        .collect::<String>();
    let stage_mask_body = if mask_stage_count == 0 {
        "    return 1.0;\n".to_owned()
    } else if stage_count.is_some() {
        (0..mask_stage_count)
            .map(|stage| {
                let slot = stage + 1;
                if stage + 1 == mask_stage_count {
                    format!("    return texture(g_Texture{slot}, uv).r;\n")
                } else {
                    format!("    if (stage == {stage}) return texture(g_Texture{slot}, uv).r;\n")
                }
            })
            .collect::<String>()
    } else {
        (0..9)
            .map(|stage| {
                let slot = stage + 1;
                if stage == 8 {
                    format!("    return texture(g_Texture{slot}, uv).r;\n")
                } else {
                    format!("    if (stage == {stage}) return texture(g_Texture{slot}, uv).r;\n")
                }
            })
            .collect::<String>()
    };
    let premultiply = premultiply_output
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or_default();
    let stage_count_declaration = stage_count.map_or_else(
        || "    int stage_count = clamp(int(u_Effect.g_Chain.x + 0.5), 0, 9);\n".to_owned(),
        |count| format!("    int stage_count = {count};\n"),
    );
    // Consecutive effect passes ping-pong between equal-sized image-local
    // targets. At zero authored displacement their fullscreen texel grids
    // coincide, so one native sample is the authored identity. Mesh and
    // puppet composites target scene color and do not have that invariant.
    let track_effect_displacement =
        fullscreen_effect && !static_black_output && stage_count != Some(2);
    let stage_evaluation = stage_count.map_or_else(
        || {
            if track_effect_displacement {
                "    bool any_displacement = false;\n\
                 for (int stage = 8; stage >= 0; --stage) {\n\
                     if (stage < stage_count) {\n\
                         vec2 stage_offset = stageOffset(stage, source_uv);\n\
                         source_uv += stage_offset;\n\
                         any_displacement = any_displacement\n\
                             || any(notEqual(stage_offset, vec2(0.0)));\n\
                     }\n\
                 }\n"
                .to_owned()
            } else {
                "    for (int stage = 8; stage >= 0; --stage) {\n\
                     if (stage < stage_count) {\n\
                         source_uv += stageOffset(stage, source_uv);\n\
                     }\n\
                 }\n"
                .to_owned()
            }
        },
        |count| {
            let declaration = track_effect_displacement
                .then_some("    bool any_displacement = false;\n")
                .unwrap_or_default();
            let stages = (0..count)
                .rev()
                .map(|stage| {
                    if track_effect_displacement {
                        format!(
                            "    vec2 stage_offset_{stage} = stageOffset({stage}, source_uv);\n\
                             source_uv += stage_offset_{stage};\n\
                             any_displacement = any_displacement\n\
                                 || any(notEqual(stage_offset_{stage}, vec2(0.0)));\n"
                        )
                    } else {
                        format!("    source_uv += stageOffset({stage}, source_uv);\n")
                    }
                })
                .collect::<String>();
            format!("{declaration}{stages}")
        },
    );
    let source_filter = if static_black_output {
        // Authored black shadows only consume source alpha. Retain the source
        // texture's implicit mip and anisotropy contract while avoiding RGB work.
        "    float source_alpha = texture(g_Texture0, source_uv).a;\n".to_owned()
    } else if stage_count == Some(2) {
        // The 0.17 texel two-stage reconstruction passes the authored temporal
        // gate with the native sample; retain implicit mip and anisotropy state.
        "    vec4 source_color = texture(g_Texture0, source_uv);\n".to_owned()
    } else if track_effect_displacement {
        r#"    vec4 source_color;
    if (!any_displacement) {
        source_color = texture(g_Texture0, source_uv);
    } else {
        vec2 source_texel = 1.0 / vec2(textureSize(g_Texture0, 0));
        vec2 filter_offset = source_texel * authored_filter_radius;
        source_color = (
            texture(g_Texture0, source_uv + vec2(-filter_offset.x, -filter_offset.y))
            + texture(g_Texture0, source_uv + vec2(filter_offset.x, -filter_offset.y))
            + texture(g_Texture0, source_uv + vec2(-filter_offset.x, filter_offset.y))
            + texture(g_Texture0, source_uv + vec2(filter_offset.x, filter_offset.y))) * 0.25;
    }
"#
        .to_owned()
    } else {
        r#"    vec2 source_texel = 1.0 / vec2(textureSize(g_Texture0, 0));
    vec2 filter_offset = source_texel * authored_filter_radius;
    vec4 source_color = (
        texture(g_Texture0, source_uv + vec2(-filter_offset.x, -filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(filter_offset.x, -filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(-filter_offset.x, filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(filter_offset.x, filter_offset.y))) * 0.25;
"#
        .to_owned()
    };
    let color_reconstruction = if static_black_output {
        r#"    vec4 color = vec4(0.0, 0.0, 0.0,
        source_alpha * u_Effect.g_ResolvedColorAlpha.a);
"#
    } else {
        "    vec4 color = source_color * u_Effect.g_ResolvedColorAlpha;\n"
    };
    let shaped_sine_declaration = r#"float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    if (exponent == 1.0) return wave;
    if (exponent == 2.0) return wave * abs(wave);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}
"#;
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
"#,
        &mask_sampler_declarations,
        r#"
layout(set = 0, binding = 3) uniform WaterWavesDirectUniform {
    vec4 g_ResolvedColorAlpha;
    vec4 g_Chain;
    vec4 g_Stage[36];
} u_Effect;
"#,
        shaped_sine_declaration,
        r#"
float stageMask(int stage, vec2 uv) {
"#,
        &stage_mask_body,
        r#"}
vec2 stageOffset(int stage, vec2 uv) {
    int base = stage * 4;
    vec4 phase_scale_strength2_mask = u_Effect.g_Stage[base];
    vec4 direction_phase2_scale2 = u_Effect.g_Stage[base + 1];
    vec4 direction2_exponents = u_Effect.g_Stage[base + 2];
    vec4 mask_resolution = u_Effect.g_Stage[base + 3];
    if (phase_scale_strength2_mask.z <= 0.0) return vec2(0.0);
    float mask = 1.0;
    if (phase_scale_strength2_mask.w > 0.5) {
        vec2 mask_uv = uv * mask_resolution.zw
            / max(mask_resolution.xy, vec2(1.0));
        mask = stageMask(stage, mask_uv);
    }
    vec2 direction = direction_phase2_scale2.xy;
    float phase = phase_scale_strength2_mask.x
        + dot(uv, direction) * phase_scale_strength2_mask.y;
    float displacement = shapedSine(phase, direction2_exponents.z);
    if (direction_phase2_scale2.w > 0.0) {
        vec2 direction2 = direction2_exponents.xy;
        float phase2 = direction_phase2_scale2.z
            + dot(uv, direction2) * direction_phase2_scale2.w;
        displacement *= shapedSine(phase2, direction2_exponents.w);
    }
    return vec2(direction.y, -direction.x)
        * displacement * phase_scale_strength2_mask.z * mask;
}
void main() {
    vec2 source_uv = v_TexCoord;
"#,
        &stage_count_declaration,
        &stage_evaluation,
        r#"
    float authored_filter_radius = 0.17
        * sqrt(max(float(min(stage_count, 7) - 1), 0.0));
"#,
        &source_filter,
        "\n",
        color_reconstruction,
        "    color.a *= v_VertexAlpha;\n",
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}

pub(crate) fn stage_count_from_shader_key(key: &str) -> Option<usize> {
    key.split("__")
        .find_map(|part| part.strip_prefix("STAGES_"))
        .and_then(|count| count.parse::<usize>().ok())
        .filter(|count| (2..=9).contains(count))
}

pub(crate) fn static_black_output_from_shader_key(key: &str) -> bool {
    key.split("__").any(|part| part == "STATIC_BLACK_1")
}
