//! Procedural GPU billboard expansion for retained particle emitters.

pub(crate) fn generic_particle_vertex_source() -> String {
    r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(location = 2) out vec3 v_ParticleColor;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
layout(set = 0, binding = 3) uniform ParticleMaterial {
    vec4 g_Color4;
    vec4 g_TimeStartRateCount;
    vec4 g_SequenceLifetimeSize;
    vec4 g_SizeFadeSeed;
    vec4 g_EmitterOrigin;
    vec4 g_EmitterDirections;
    vec4 g_DistanceMin;
    vec4 g_DistanceMax;
    vec4 g_VelocityMin;
    vec4 g_VelocityMax;
    vec4 g_Gravity;
    vec4 g_Turbulence;
    vec4 g_AngularVelocityMin;
    vec4 g_AngularVelocityMax;
    vec4 g_RotationColorMix;
    vec4 g_ColorMin;
    vec4 g_ColorMax;
} g_Particle;

float hash11(float value) {
    return fract(sin(value * 12.9898 + 78.233) * 43758.5453);
}

vec3 hash31(float seed) {
    return vec3(hash11(seed + 1.0), hash11(seed + 17.0), hash11(seed + 53.0));
}

vec2 billboard_corner(int vertex) {
    const vec2 corners[6] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    return corners[vertex];
}

vec2 billboard_uv(int vertex) {
    const vec2 uvs[6] = vec2[](
        vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0));
    return uvs[vertex];
}

void main() {
    float instance = float(gl_InstanceIndex);
    float base_seed = g_Particle.g_SizeFadeSeed.w + instance * 131.0;
    vec3 initial_random = hash31(base_seed);
    float lifetime = mix(
        g_Particle.g_SequenceLifetimeSize.y,
        g_Particle.g_SequenceLifetimeSize.z,
        initial_random.x);
    float spawn_offset = instance / max(g_Particle.g_TimeStartRateCount.z, 0.0001);
    float elapsed = g_Particle.g_TimeStartRateCount.x
        - g_Particle.g_TimeStartRateCount.y - spawn_offset;
    float cycle = floor(max(elapsed, 0.0) / max(lifetime, 0.0001));
    float age = mod(max(elapsed, 0.0), max(lifetime, 0.0001));
    float alive = elapsed >= 0.0 ? 1.0 : 0.0;
    float seed = base_seed + cycle * 977.0;
    vec3 random0 = hash31(seed);
    vec3 random1 = hash31(seed + 101.0);
    vec3 random2 = hash31(seed + 211.0);

    float radius = mix(
        g_Particle.g_DistanceMin.x,
        g_Particle.g_DistanceMax.x,
        pow(random0.x, 0.3333333));
    float azimuth = random0.y * 6.2831853;
    float elevation = mix(-1.0, 1.0, random0.z);
    float radial_xy = sqrt(max(1.0 - elevation * elevation, 0.0));
    vec3 sphere_direction = vec3(
        cos(azimuth) * radial_xy,
        sin(azimuth) * radial_xy,
        elevation) * g_Particle.g_EmitterDirections.xyz;
    vec3 position = g_Particle.g_EmitterOrigin.xyz + sphere_direction * radius;
    vec3 velocity = mix(
        g_Particle.g_VelocityMin.xyz,
        g_Particle.g_VelocityMax.xyz,
        random1);
    position += velocity * age + g_Particle.g_Gravity.xyz * (0.5 * age * age);

    float turbulent_speed = mix(
        g_Particle.g_Turbulence.z,
        g_Particle.g_Turbulence.w,
        random2.x);
    float turbulent_phase = seed * 0.017 + age * g_Particle.g_Turbulence.x;
    vec2 turbulent_direction = vec2(sin(turbulent_phase), cos(turbulent_phase * 0.73));
    position.xy += turbulent_direction * turbulent_speed
        * g_Particle.g_Turbulence.y * age * 0.2;

    float size = mix(
        g_Particle.g_SequenceLifetimeSize.w,
        g_Particle.g_SizeFadeSeed.x,
        random2.y);
    float rotation = mix(
        g_Particle.g_RotationColorMix.x,
        g_Particle.g_RotationColorMix.y,
        random2.z);
    float angular_velocity = mix(
        g_Particle.g_AngularVelocityMin.z,
        g_Particle.g_AngularVelocityMax.z,
        random1.z);
    rotation += angular_velocity * age;
    vec2 corner = billboard_corner(gl_VertexIndex) * size * 0.5;
    float sine = sin(rotation);
    float cosine = cos(rotation);
    corner = mat2(cosine, -sine, sine, cosine) * corner;

    float lifetime_fraction = age / max(lifetime, 0.0001);
    float fade_in = g_Particle.g_SizeFadeSeed.y <= 0.0
        ? 1.0
        : clamp(lifetime_fraction / g_Particle.g_SizeFadeSeed.y, 0.0, 1.0);
    float fade_out_width = max(1.0 - g_Particle.g_SizeFadeSeed.z, 0.0001);
    float fade_out = clamp((1.0 - lifetime_fraction) / fade_out_width, 0.0, 1.0);
    v_VertexAlpha = alive * min(fade_in, fade_out);
    v_ParticleColor = mix(
        g_Particle.g_ColorMin.xyz,
        g_Particle.g_ColorMax.xyz,
        random0);
    v_TexCoord = billboard_uv(gl_VertexIndex);

    vec4 local_position = vec4(position.xy + corner, position.z, 1.0);
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
}
"#
    .to_owned()
}
