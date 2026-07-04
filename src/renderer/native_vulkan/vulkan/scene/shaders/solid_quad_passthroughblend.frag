#version 450

layout(location = 0) in vec4 v_color;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Framebuffer;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
} pc;

vec3 rgb_to_hsl(vec3 color) {
    float fmin = min(min(color.r, color.g), color.b);
    float fmax = max(max(color.r, color.g), color.b);
    float delta = fmax - fmin;
    vec3 hsl = vec3(0.0, 0.0, (fmax + fmin) * 0.5);
    if (delta == 0.0) {
        return hsl;
    }
    hsl.y = hsl.z < 0.5 ? delta / (fmax + fmin) : delta / (2.0 - fmax - fmin);
    float delta_r = (((fmax - color.r) / 6.0) + (delta * 0.5)) / delta;
    float delta_g = (((fmax - color.g) / 6.0) + (delta * 0.5)) / delta;
    float delta_b = (((fmax - color.b) / 6.0) + (delta * 0.5)) / delta;
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
    return hsl;
}

float hue_to_rgb(float f1, float f2, float hue) {
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

vec3 hsl_to_rgb(vec3 hsl) {
    if (hsl.y == 0.0) {
        return vec3(hsl.z);
    }
    float f2 = hsl.z < 0.5 ? hsl.z * (1.0 + hsl.y) : (hsl.z + hsl.y) - (hsl.y * hsl.z);
    float f1 = 2.0 * hsl.z - f2;
    return vec3(
        hue_to_rgb(f1, f2, hsl.x + (1.0 / 3.0)),
        hue_to_rgb(f1, f2, hsl.x),
        hue_to_rgb(f1, f2, hsl.x - (1.0 / 3.0))
    );
}

vec3 blend_color(vec3 base, vec3 blend) {
    vec3 blend_hsl = rgb_to_hsl(blend);
    return hsl_to_rgb(vec3(blend_hsl.r, blend_hsl.g, rgb_to_hsl(base).b));
}

vec3 blend_hue(vec3 base, vec3 blend) {
    vec3 base_hsl = rgb_to_hsl(base);
    return hsl_to_rgb(vec3(rgb_to_hsl(blend).r, base_hsl.g, base_hsl.b));
}

vec3 blend_saturation(vec3 base, vec3 blend) {
    vec3 base_hsl = rgb_to_hsl(base);
    return hsl_to_rgb(vec3(base_hsl.r, rgb_to_hsl(blend).g, base_hsl.b));
}

vec3 blend_luminosity(vec3 base, vec3 blend) {
    vec3 base_hsl = rgb_to_hsl(base);
    return hsl_to_rgb(vec3(base_hsl.r, base_hsl.g, rgb_to_hsl(blend).b));
}

vec3 blend_screen(vec3 base, vec3 blend) {
    return 1.0 - ((1.0 - base) * (1.0 - blend));
}

vec3 blend_overlay(vec3 base, vec3 blend) {
    return mix(
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend),
        2.0 * base * blend,
        lessThan(base, vec3(0.5))
    );
}

float blend_soft_light_f(float base, float blend) {
    if (blend < 0.5) {
        return 2.0 * base * blend + base * base * (1.0 - 2.0 * blend);
    }
    return sqrt(base) * (2.0 * blend - 1.0) + 2.0 * base * (1.0 - blend);
}

vec3 blend_soft_light(vec3 base, vec3 blend) {
    return vec3(
        blend_soft_light_f(base.r, blend.r),
        blend_soft_light_f(base.g, blend.g),
        blend_soft_light_f(base.b, blend.b)
    );
}

float blend_color_dodge_f(float base, float blend) {
    return blend == 1.0 ? blend : min(base / (1.0 - blend), 1.0);
}

vec3 blend_color_dodge(vec3 base, vec3 blend) {
    return vec3(
        blend_color_dodge_f(base.r, blend.r),
        blend_color_dodge_f(base.g, blend.g),
        blend_color_dodge_f(base.b, blend.b)
    );
}

float blend_color_burn_f(float base, float blend) {
    return blend == 0.0 ? blend : max(1.0 - ((1.0 - base) / blend), 0.0);
}

vec3 blend_color_burn(vec3 base, vec3 blend) {
    return vec3(
        blend_color_burn_f(base.r, blend.r),
        blend_color_burn_f(base.g, blend.g),
        blend_color_burn_f(base.b, blend.b)
    );
}

vec3 blend_linear_light(vec3 base, vec3 blend) {
    vec3 burn = max(base + (2.0 * blend) - 1.0, vec3(0.0));
    vec3 dodge = base + (2.0 * (blend - 0.5));
    return mix(dodge, burn, lessThan(blend, vec3(0.5)));
}

vec3 blend_vivid_light(vec3 base, vec3 blend) {
    vec3 burn = blend_color_burn(base, 2.0 * blend);
    vec3 dodge = blend_color_dodge(base, 2.0 * (blend - 0.5));
    return mix(dodge, burn, lessThan(blend, vec3(0.5)));
}

vec3 blend_pin_light(vec3 base, vec3 blend) {
    vec3 darken = min(2.0 * blend, base);
    vec3 lighten = max(2.0 * (blend - 0.5), base);
    return mix(lighten, darken, lessThan(blend, vec3(0.5)));
}

vec3 blend_hard_mix(vec3 base, vec3 blend) {
    return step(vec3(0.5), blend_vivid_light(base, blend));
}

float blend_reflect_f(float base, float blend) {
    return blend == 1.0 ? blend : min(base * base / (1.0 - blend), 1.0);
}

vec3 blend_reflect(vec3 base, vec3 blend) {
    return vec3(
        blend_reflect_f(base.r, blend.r),
        blend_reflect_f(base.g, blend.g),
        blend_reflect_f(base.b, blend.b)
    );
}

vec3 blend_phoenix(vec3 base, vec3 blend) {
    return min(base, blend) - max(base, blend) + vec3(1.0);
}

vec3 blend_tint(vec3 base, vec3 blend) {
    return vec3(max(base.r, max(base.g, base.b))) * blend;
}

vec3 apply_blending(uint mode, vec3 screen, vec3 albedo, float opacity) {
    if (mode == 1u) {
        return mix(screen, min(albedo, screen), opacity);
    }
    if (mode == 2u) {
        return mix(screen, screen * albedo, opacity);
    }
    if (mode == 3u) {
        return mix(screen, blend_color_burn(screen, albedo), opacity);
    }
    if (mode == 4u || mode == 20u) {
        return mix(screen, max(screen + albedo - vec3(1.0), vec3(0.0)), opacity);
    }
    if (mode == 5u) {
        return min(screen, albedo);
    }
    if (mode == 6u) {
        return mix(screen, max(albedo, screen), opacity);
    }
    if (mode == 7u) {
        return mix(screen, blend_screen(screen, albedo), opacity);
    }
    if (mode == 8u) {
        return mix(screen, blend_color_dodge(screen, albedo), opacity);
    }
    if (mode == 9u) {
        return mix(screen, min(screen + albedo, vec3(1.0)), opacity);
    }
    if (mode == 10u) {
        return max(screen, albedo);
    }
    if (mode == 11u) {
        return mix(screen, blend_overlay(screen, albedo), opacity);
    }
    if (mode == 12u) {
        return mix(screen, blend_soft_light(screen, albedo), opacity);
    }
    if (mode == 13u) {
        return mix(screen, blend_overlay(albedo, screen), opacity);
    }
    if (mode == 14u) {
        return mix(screen, blend_vivid_light(screen, albedo), opacity);
    }
    if (mode == 15u) {
        return mix(screen, blend_linear_light(screen, albedo), opacity);
    }
    if (mode == 16u) {
        return mix(screen, blend_pin_light(screen, albedo), opacity);
    }
    if (mode == 17u) {
        return mix(screen, blend_hard_mix(screen, albedo), opacity);
    }
    if (mode == 18u) {
        return mix(screen, abs(screen - albedo), opacity);
    }
    if (mode == 19u) {
        return mix(screen, screen + albedo - 2.0 * screen * albedo, opacity);
    }
    if (mode == 21u) {
        return mix(screen, blend_reflect(screen, albedo), opacity);
    }
    if (mode == 22u) {
        return mix(screen, blend_reflect(albedo, screen), opacity);
    }
    if (mode == 23u) {
        return mix(screen, blend_phoenix(screen, albedo), opacity);
    }
    if (mode == 24u) {
        return mix(screen, (screen + albedo) * 0.5, opacity);
    }
    if (mode == 25u) {
        return mix(screen, vec3(1.0) - abs(vec3(1.0) - screen - albedo), opacity);
    }
    if (mode == 26u) {
        return mix(screen, blend_hue(screen, albedo), opacity);
    }
    if (mode == 27u) {
        return mix(screen, blend_saturation(screen, albedo), opacity);
    }
    if (mode == 28u) {
        return mix(screen, blend_color(screen, albedo), opacity);
    }
    if (mode == 29u) {
        return mix(screen, blend_luminosity(screen, albedo), opacity);
    }
    if (mode == 30u) {
        return mix(screen, blend_tint(screen, albedo), opacity);
    }
    if (mode == 31u) {
        return screen + albedo * opacity;
    }
    if (mode == 32u) {
        return mix(screen, screen + screen * albedo, opacity);
    }
    return mix(screen, albedo, opacity);
}

void main() {
    vec2 screen_uv = clamp(gl_FragCoord.xy / max(pc.extent, vec2(1.0)), vec2(0.0), vec2(1.0));
    vec4 screen = texture(g_Framebuffer, screen_uv);
    out_color.rgb = apply_blending(28u, screen.rgb, v_color.rgb, v_color.a);
    out_color.a = screen.a;
}
