//! Exact typed fusion for WE's flat -> rounded-mask -> HSL Color chain.

pub(super) fn flat_rounded_hsl_source_sources() -> (String, String) {
    let vertex = super::flat_rounded_mask_support_vertex_source();
    let fragment = r#"#version 450
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SceneSnapshot;
layout(set = 0, binding = 3) uniform FlatRoundedMaskCompositeMaterial {
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidthSourceExtent;
    vec4 g_ResolvedColorAlpha;
} u_Effect;

vec4 quantizeUnorm8(vec4 value) {
    return roundEven(clamp(value, 0.0, 1.0) * 255.0) / 255.0;
}

float roundedBoxSdf(vec2 point, vec2 size, float radius) {
    vec2 half_size = size * 0.5;
    float half_min = min(half_size.x, half_size.y);
    float r = clamp(radius * half_min, 0.001, half_min);
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}

float roundedMaskAtTexel(ivec2 texel, ivec2 source_extent) {
    ivec2 clamped_texel = clamp(texel, ivec2(0), source_extent - ivec2(1));
    vec2 object_uv = (vec2(clamped_texel) + 0.5) / vec2(source_extent);
    float width_pixels = max(v_ObjectPixelExtent.x, 1.0);
    float height_pixels = max(v_ObjectPixelExtent.y, 1.0);
    vec2 aspect_scale = vec2(max(1.0, width_pixels / height_pixels),
        max(1.0, height_pixels / width_pixels));
    vec2 mask_uv = (object_uv - 0.5) * aspect_scale + 0.5;
    vec2 mask_size = u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale;
    float distance = roundedBoxSdf(mask_uv - vec2(0.5), mask_size,
        u_Effect.g_ColorRadius.w);
    float edge_softness = u_Effect.g_SizeSoftnessAlpha.z
        / max(float(source_extent.x), float(source_extent.y)) * 2.0;
    return smoothstep(edge_softness, 0.0, distance);
}

vec4 roundedTargetTexel(ivec2 texel, ivec2 source_extent, vec4 flat_source) {
    float mask_alpha = roundedMaskAtTexel(texel, source_extent);
    float input_alpha = flat_source.a;
    float blend_weight = mask_alpha * input_alpha
        * u_Effect.g_SizeSoftnessAlpha.w;
    vec3 rounded_color = u_Effect.g_ColorRadius.rgb
        + (flat_source.rgb - u_Effect.g_ColorRadius.rgb)
            * blend_weight * input_alpha;
    return quantizeUnorm8(vec4(rounded_color, mask_alpha));
}

vec4 reconstructedRoundedSource(vec2 object_uv) {
    vec4 flat_source = quantizeUnorm8(u_Effect.g_ResolvedColorAlpha);
    if (u_Effect.g_BorderWidthSourceExtent.y <= 0.5) {
        return flat_source;
    }
    ivec2 source_extent = max(
        ivec2(round(u_Effect.g_BorderWidthSourceExtent.zw)), ivec2(1));
    vec2 texel_position = object_uv * vec2(source_extent) - 0.5;
    ivec2 base = ivec2(floor(texel_position));
    vec2 weight = fract(texel_position);
    vec4 top = mix(
        roundedTargetTexel(base, source_extent, flat_source),
        roundedTargetTexel(base + ivec2(1, 0), source_extent, flat_source),
        weight.x);
    vec4 bottom = mix(
        roundedTargetTexel(base + ivec2(0, 1), source_extent, flat_source),
        roundedTargetTexel(base + ivec2(1, 1), source_extent, flat_source),
        weight.x);
    return mix(top, bottom, weight.y);
}

// WE `common_blending.h` Color path (colorBlendMode 28 / BLENDMODE Color):
// keep source hue+saturation, destination lightness. DXBC ps-969ba0 implements the
// same RGB<->HSL round-trip; Photoshop setLum/clip is a different operator and
// desaturates the pale post-rounded UNORM source into gray.
vec3 rgbToHsl(vec3 color) {
    float fmin = min(min(color.r, color.g), color.b);
    float fmax = max(max(color.r, color.g), color.b);
    float delta = fmax - fmin;
    vec3 hsl = vec3(0.0, 0.0, (fmax + fmin) * 0.5);
    if (delta != 0.0) {
        if (hsl.z < 0.5) {
            hsl.y = delta / (fmax + fmin);
        } else {
            hsl.y = delta / (2.0 - fmax - fmin);
        }
        float half_delta = delta * 0.5;
        float delta_r = (((fmax - color.r) / 6.0) + half_delta) / delta;
        float delta_g = (((fmax - color.g) / 6.0) + half_delta) / delta;
        float delta_b = (((fmax - color.b) / 6.0) + half_delta) / delta;
        if (color.r == fmax) {
            hsl.x = delta_b - delta_g;
        } else if (color.g == fmax) {
            hsl.x = (1.0 / 3.0) + delta_r - delta_b;
        } else {
            hsl.x = (2.0 / 3.0) + delta_g - delta_r;
        }
        if (hsl.x < 0.0) {
            hsl.x += 1.0;
        } else if (hsl.x > 1.0) {
            hsl.x -= 1.0;
        }
    }
    return hsl;
}

float hueToRgb(float f1, float f2, float hue) {
    if (hue < 0.0) {
        hue += 1.0;
    } else if (hue > 1.0) {
        hue -= 1.0;
    }
    if ((6.0 * hue) < 1.0) {
        return f1 + (f2 - f1) * 6.0 * hue;
    }
    if ((2.0 * hue) < 1.0) {
        return f2;
    }
    if ((3.0 * hue) < 2.0) {
        return f1 + (f2 - f1) * ((2.0 / 3.0) - hue) * 6.0;
    }
    return f1;
}

vec3 hslToRgb(vec3 hsl) {
    if (hsl.y == 0.0) {
        return vec3(hsl.z);
    }
    float f2;
    if (hsl.z < 0.5) {
        f2 = hsl.z * (1.0 + hsl.y);
    } else {
        f2 = (hsl.z + hsl.y) - (hsl.y * hsl.z);
    }
    float f1 = 2.0 * hsl.z - f2;
    return vec3(
        hueToRgb(f1, f2, hsl.x + (1.0 / 3.0)),
        hueToRgb(f1, f2, hsl.x),
        hueToRgb(f1, f2, hsl.x - (1.0 / 3.0)));
}

// BlendColor(destination, source): source H/S + destination L.
vec3 blendColor(vec3 destination, vec3 source) {
    vec3 source_hsl = rgbToHsl(source);
    return hslToRgb(vec3(source_hsl.xy, rgbToHsl(destination).z));
}

void main() {
    vec4 source = reconstructedRoundedSource(v_ObjectTexCoord);
    vec4 destination = texelFetch(
        g_SceneSnapshot, ivec2(gl_FragCoord.xy), 0);
    // WE HSL PS: o = D + S.a*(Color(S,D)-D), a = D.a; then SRC_ALPHA/INV_SRC_ALPHA
    // RGB-only yields fused D + D.a*S.a*(Color-D) with destination alpha retained.
    vec3 blended = blendColor(destination.rgb, source.rgb);
    vec3 result = destination.rgb + destination.a * source.a
        * (blended - destination.rgb);
    o_Color = vec4(result, destination.a);
}
"#;
    (vertex, fragment.to_owned())
}
