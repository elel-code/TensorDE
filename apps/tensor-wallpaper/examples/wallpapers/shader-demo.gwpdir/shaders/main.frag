#ifdef GL_ES
precision mediump float;
#endif

uniform float u_time;
uniform vec2 u_resolution;
uniform vec2 u_mouse;
uniform float u_intensity;

void main() {
    vec2 safe_resolution = max(u_resolution, vec2(1.0));
    vec2 uv = gl_FragCoord.xy / safe_resolution;
    vec2 mouse = u_mouse / safe_resolution;
    float wave = sin((uv.x + u_time * 0.08) * 18.0) * 0.5 + 0.5;
    float focus = 1.0 - smoothstep(0.0, 0.75, distance(uv, mouse));
    vec3 base = mix(vec3(0.04, 0.10, 0.16), vec3(0.20, 0.52, 0.74), uv.y);
    vec3 accent = vec3(0.90, 0.68, 0.24) * wave * u_intensity;
    vec3 cursor = vec3(0.95, 0.96, 0.88) * focus * 0.35;
    gl_FragColor = vec4(base + accent + cursor, 1.0);
}
