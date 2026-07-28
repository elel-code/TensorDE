#version 460

layout(set = 0, binding = 0) uniform texture2D text_atlas;
layout(set = 0, binding = 1) uniform sampler text_sampler;

layout(location = 0) in vec2 in_uv;
layout(location = 1) in vec4 in_color;

layout(location = 0) out vec4 out_color;

void main() {
    float mask = texture(sampler2D(text_atlas, text_sampler), in_uv).r;
    out_color = vec4(in_color.rgb, in_color.a * mask);
}
