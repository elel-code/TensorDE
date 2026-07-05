#version 450
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_buffer_reference2 : require
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require

struct SolidVertex {
    vec2 position;
    vec4 rgba;
};

struct FrameTime {
    vec4 constants;
};

layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer SolidVertexTimelineBuffer {
    float values[];
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer FrameTimeBuffer {
    FrameTime frame;
};

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec4 in_rgba;
layout(location = 2) in uvec4 in_vertex_timeline_ref;
layout(location = 3) in uvec4 in_frame_time_ref;

layout(location = 0) out vec4 v_rgba;

layout(push_constant) uniform ScenePush {
    vec2 extent;
} pc;

SolidVertexTimelineBuffer vertex_timeline_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return SolidVertexTimelineBuffer(address);
}

FrameTimeBuffer frame_time_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return FrameTimeBuffer(address);
}

bool has_vertex_timeline_ref() {
    return in_vertex_timeline_ref.x != 0u || in_vertex_timeline_ref.y != 0u;
}

float frame_time_seconds() {
    if (in_frame_time_ref.x == 0u && in_frame_time_ref.y == 0u) {
        return 0.0;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.x;
}

SolidVertex load_solid_vertex(SolidVertexTimelineBuffer timeline, uint vertex_index);

SolidVertex solid_vertex_at_time() {
    SolidVertexTimelineBuffer timeline = vertex_timeline_buffer_from_ref(in_vertex_timeline_ref);
    uint frame_count = max(in_vertex_timeline_ref.z, 1u);
    float frame_rate = max(float(in_vertex_timeline_ref.w), 1.0);
    uint frame_vertex_count = max(in_frame_time_ref.z, 1u);
    uint vertex_index = min(uint(gl_VertexIndex), frame_vertex_count - 1u);
    float frame = min(max(frame_time_seconds() * frame_rate, 0.0), float(frame_count - 1u));
    uint frame0 = min(uint(floor(frame)), frame_count - 1u);
    uint frame1 = min(frame0 + 1u, frame_count - 1u);
    float frame_mix = fract(frame);
    SolidVertex a = load_solid_vertex(timeline, frame0 * frame_vertex_count + vertex_index);
    SolidVertex b = load_solid_vertex(timeline, frame1 * frame_vertex_count + vertex_index);
    SolidVertex vertex;
    vertex.position = mix(a.position, b.position, frame_mix);
    vertex.rgba = mix(a.rgba, b.rgba, frame_mix);
    return vertex;
}

SolidVertex load_solid_vertex(SolidVertexTimelineBuffer timeline, uint vertex_index) {
    uint base = vertex_index * 6u;
    SolidVertex vertex;
    vertex.position = vec2(timeline.values[base], timeline.values[base + 1u]);
    vertex.rgba = vec4(
        timeline.values[base + 2u],
        timeline.values[base + 3u],
        timeline.values[base + 4u],
        timeline.values[base + 5u]
    );
    return vertex;
}

void main() {
    vec2 position = in_position;
    vec4 rgba = in_rgba;
    if (has_vertex_timeline_ref()) {
        SolidVertex vertex = solid_vertex_at_time();
        position = vertex.position;
        rgba = vertex.rgba;
    }
    vec2 normalized = position / pc.extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_rgba = rgba;
}
