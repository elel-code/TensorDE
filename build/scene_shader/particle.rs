//! Procedural GPU billboard expansion for retained particle emitters.

pub(crate) fn generic_particle_vertex_source() -> String {
    r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(location = 2) out vec3 v_ParticleColor;
layout(location = 3) flat out float v_TextureDecodeMode;
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
    vec4 g_SimulationOscillation;
    vec4 g_OscillationScale;
    vec4 g_BillboardInverse;
    vec4 g_BillboardTexture;
    vec4 g_PositionOscillationFrequencyPhase;
    vec4 g_PositionOscillationScaleMask;
    vec4 g_SizeOscillationFrequencyPhase;
    vec4 g_SizeOscillationScaleMaskZ;
} g_Particle;

float hash11(float value) {
    return fract(sin(value * 12.9898 + 78.233) * 43758.5453);
}

vec3 hash31(float seed) {
    return vec3(hash11(seed + 1.0), hash11(seed + 17.0), hash11(seed + 53.0));
}

uint hash_u32(uint state) {
    state ^= state >> 16u;
    state *= 0x7feb352du;
    state ^= state >> 15u;
    state *= 0x846ca68bu;
    state ^= state >> 16u;
    return state;
}

float random_u32(uint state) {
    return float(hash_u32(state)) * (1.0 / 4294967296.0);
}

vec3 random3_u32(uint state) {
    return vec3(
        random_u32(state ^ 0x68bc21ebu),
        random_u32(state ^ 0x02e5be93u),
        random_u32(state ^ 0x967a889bu));
}

vec2 billboard_corner(int vertex) {
    const vec2 corners[4] = vec2[](
        vec2(-1.0, -1.0), vec2(1.0, -1.0),
        vec2(-1.0, 1.0), vec2(1.0, 1.0));
    return corners[vertex];
}

vec2 billboard_uv(int vertex) {
    const vec2 uvs[4] = vec2[](
        vec2(0.0, 1.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 0.0));
    return uvs[vertex];
}

void main() {
    float instance = float(gl_InstanceIndex);
    float base_seed = g_Particle.g_SizeFadeSeed.w + instance * 131.0;
    float lifetime_min = g_Particle.g_SequenceLifetimeSize.y;
    float lifetime_max = g_Particle.g_SequenceLifetimeSize.z;
    float initial_lifetime = lifetime_min;
    if (lifetime_min != lifetime_max) {
        initial_lifetime = mix(lifetime_min, lifetime_max, hash11(base_seed + 1.0));
    }
    float spawn_offset = instance / max(g_Particle.g_TimeStartRateCount.z, 0.0001);
    float elapsed = g_Particle.g_TimeStartRateCount.x
        - g_Particle.g_TimeStartRateCount.y - spawn_offset;
    float requested_slots = ceil(
        g_Particle.g_TimeStartRateCount.z * g_Particle.g_SequenceLifetimeSize.z);
    float active_slots = min(
        g_Particle.g_TimeStartRateCount.w,
        requested_slots);
    bool capacity_saturated = g_Particle.g_TimeStartRateCount.w < requested_slots;
    // A non-saturated emitter owns enough slots for every emission in the longest
    // lifetime window. Reuse each slot only after capacity/rate seconds: a particle
    // whose random lifetime ended stays dead until that next authored emission.
    // Saturated emitters instead reuse a slot on death because maxcount throttles rate.
    float schedule_period = capacity_saturated
        ? initial_lifetime
        : active_slots / max(g_Particle.g_TimeStartRateCount.z, 0.0001);
    float cycle = floor(max(elapsed, 0.0) / max(schedule_period, 0.0001));
    float age = mod(max(elapsed, 0.0), max(schedule_period, 0.0001));
    float seed = base_seed + cycle * 977.0;
    float lifetime = initial_lifetime;
    if (!capacity_saturated && lifetime_min != lifetime_max) {
        lifetime = mix(lifetime_min, lifetime_max, hash11(seed + 1.0));
    }
    float alive = elapsed >= 0.0
        && instance < active_slots
        && (capacity_saturated || age < lifetime) ? 1.0 : 0.0;

    if (alive < 0.5) {
        v_TexCoord = vec2(0.0);
        v_VertexAlpha = 0.0;
        v_ParticleColor = vec3(0.0);
        v_TextureDecodeMode = 0.0;
        gl_Position = vec4(2.0, 2.0, 0.0, 1.0);
        return;
    }

    float simulation = g_Particle.g_SimulationOscillation.x;
    // Each WE particle module advances its own stable random selection. Keep the
    // ambient-particle fields independent so position cannot bias size, color, or
    // alpha oscillation. The sphere path retains its established random stream.
    uint ambient_seed = uint(max(g_Particle.g_SizeFadeSeed.w, 0.0) + 0.5)
        ^ (uint(gl_InstanceIndex) * 0x9e3779b9u)
        ^ (uint(max(cycle, 0.0)) * 0x85ebca6bu);
    vec3 box_direction_random = random3_u32(ambient_seed ^ 0x307a7ca5u);
    vec3 ambient_velocity_random = random3_u32(ambient_seed ^ 0x503d4567u);
    vec3 ambient_turbulence_random = random3_u32(ambient_seed ^ 0x601e8968u);
    vec3 ambient_shape_random = random3_u32(ambient_seed ^ 0x701a29f9u);
    vec3 ambient_color_random = random3_u32(ambient_seed ^ 0x809b4a7au);
    float ambient_oscillation_random = random_u32(ambient_seed ^ 0x907c6b0bu);
    vec3 floral_position_frequency_random = random3_u32(ambient_seed ^ 0xa0f71c29u);
    vec3 floral_position_phase_random = random3_u32(ambient_seed ^ 0xb0986e43u);
    vec3 floral_position_scale_random = random3_u32(ambient_seed ^ 0xc0472ad1u);
    float floral_size_random = random_u32(ambient_seed ^ 0xd086c72fu);
    vec3 random0 = vec3(0.0);
    vec3 random1 = vec3(0.0);
    vec3 random2 = vec3(0.0);
    if (simulation <= 1.5) {
        random0 = hash31(seed);
        random1 = hash31(seed + 101.0);
        random2 = hash31(seed + 211.0);
    }
    vec3 position;
    if (simulation > 1.5) {
        vec3 box_direction = (2.0 * box_direction_random - 1.0)
            * g_Particle.g_EmitterDirections.xyz;
        vec3 box_distance = g_Particle.g_DistanceMin.xyz
            + abs(box_direction)
                * (g_Particle.g_DistanceMax.xyz - g_Particle.g_DistanceMin.xyz);
        position = g_Particle.g_EmitterOrigin.xyz
            + sign(box_direction) * box_distance;
    } else {
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
        position = g_Particle.g_EmitterOrigin.xyz + sphere_direction * radius;
    }
    vec3 velocity = mix(
        g_Particle.g_VelocityMin.xyz,
        g_Particle.g_VelocityMax.xyz,
        simulation > 1.5 ? ambient_velocity_random : random1);
    position += velocity * age + g_Particle.g_Gravity.xyz * (0.5 * age * age);

    float turbulent_speed = mix(
        g_Particle.g_Turbulence.z,
        g_Particle.g_Turbulence.w,
        simulation > 1.5 ? ambient_turbulence_random.x : random2.x);
    float turbulent_phase = seed * 0.017 + age * g_Particle.g_Turbulence.x;
    vec2 turbulent_direction = vec2(sin(turbulent_phase), cos(turbulent_phase * 0.73));
    position.xy += turbulent_direction * turbulent_speed
        * g_Particle.g_Turbulence.y * age * 0.2;

    if (simulation > 2.5) {
        vec2 frequency = mix(
            vec2(g_Particle.g_PositionOscillationFrequencyPhase.x),
            vec2(g_Particle.g_PositionOscillationFrequencyPhase.y),
            floral_position_frequency_random.xy);
        vec2 phase = mix(
            vec2(g_Particle.g_PositionOscillationFrequencyPhase.z),
            vec2(g_Particle.g_PositionOscillationFrequencyPhase.w),
            floral_position_phase_random.xy);
        vec2 amplitude = mix(
            vec2(g_Particle.g_PositionOscillationScaleMask.x),
            vec2(g_Particle.g_PositionOscillationScaleMask.y),
            floral_position_scale_random.xy);
        vec2 displacement = amplitude
            * (cos(frequency * age + phase) - cos(phase));
        position.xy += displacement * g_Particle.g_PositionOscillationScaleMask.zw;
    }

    float size = mix(
        g_Particle.g_SequenceLifetimeSize.w,
        g_Particle.g_SizeFadeSeed.x,
        simulation > 1.5 ? ambient_shape_random.x : random2.y);
    if (simulation > 2.5) {
        float frequency = mix(
            g_Particle.g_SizeOscillationFrequencyPhase.x,
            g_Particle.g_SizeOscillationFrequencyPhase.y,
            floral_size_random);
        float phase = mix(
            g_Particle.g_SizeOscillationFrequencyPhase.z,
            g_Particle.g_SizeOscillationFrequencyPhase.w,
            floral_size_random);
        float pulse = 0.5 + 0.5 * sin(frequency * (age + phase));
        float scale_delta = g_Particle.g_SizeOscillationScaleMaskZ.y
            - g_Particle.g_SizeOscillationScaleMaskZ.x;
        size *= g_Particle.g_SizeOscillationScaleMaskZ.x
            + floral_size_random * scale_delta * pulse;
    }
    float rotation = mix(
        g_Particle.g_RotationColorMix.x,
        g_Particle.g_RotationColorMix.y,
        simulation > 1.5 ? ambient_shape_random.y : random2.z);
    float angular_velocity = mix(
        g_Particle.g_AngularVelocityMin.z,
        g_Particle.g_AngularVelocityMax.z,
        simulation > 1.5 ? ambient_shape_random.z : random1.z);
    rotation += angular_velocity * age;
    vec2 screen_corner = billboard_corner(gl_VertexIndex)
        * vec2(1.0, g_Particle.g_BillboardTexture.x) * size * 0.5;
    float sine = sin(rotation);
    float cosine = cos(rotation);
    screen_corner = vec2(
        cosine * screen_corner.x + sine * screen_corner.y,
        -sine * screen_corner.x + cosine * screen_corner.y);
    vec2 corner = vec2(
        g_Particle.g_BillboardInverse.x * screen_corner.x
            + g_Particle.g_BillboardInverse.y * screen_corner.y,
        g_Particle.g_BillboardInverse.z * screen_corner.x
            + g_Particle.g_BillboardInverse.w * screen_corner.y);

    float lifetime_fraction = age / max(lifetime, 0.0001);
    float fade_in = g_Particle.g_SizeFadeSeed.y <= 0.0
        ? 1.0
        : clamp(lifetime_fraction / g_Particle.g_SizeFadeSeed.y, 0.0, 1.0);
    float fade_out_width = max(1.0 - g_Particle.g_SizeFadeSeed.z, 0.0001);
    float fade_out = clamp((1.0 - lifetime_fraction) / fade_out_width, 0.0, 1.0);
    float opacity_oscillation = 1.0;
    if (simulation > 1.5 && simulation < 2.5) {
        float oscillator_random = ambient_oscillation_random;
        float frequency = mix(
            g_Particle.g_SimulationOscillation.y,
            g_Particle.g_SimulationOscillation.z,
            oscillator_random);
        float phase = mix(
            g_Particle.g_OscillationScale.z,
            g_Particle.g_OscillationScale.w,
            oscillator_random);
        float pulse = 0.5 + 0.5 * sin(frequency * (age + phase));
        float scale_delta = g_Particle.g_OscillationScale.x
            - g_Particle.g_SimulationOscillation.w;
        opacity_oscillation = g_Particle.g_SimulationOscillation.w
            + oscillator_random * scale_delta * pulse;
    }
    v_VertexAlpha = alive * min(fade_in, fade_out) * opacity_oscillation;
    v_ParticleColor = mix(
        g_Particle.g_ColorMin.xyz,
        g_Particle.g_ColorMax.xyz,
        simulation > 1.5 ? ambient_color_random : random0);
    v_TextureDecodeMode = g_Particle.g_OscillationScale.y;
    v_TexCoord = billboard_uv(gl_VertexIndex);

    // WE's orthographic scene particles simulate a three-component position, but the
    // authored Z is not Vulkan clip-space depth. Billboard ordering is owned by the
    // scene graph, so projecting the random sphere Z would clip almost every 2D leaf.
    vec4 local_position = vec4(position.xy + corner, 0.0, 1.0);
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
}
"#
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::generic_particle_vertex_source;

    fn native_box_offset(random: f32, direction: f32, minimum: f32, maximum: f32) -> f32 {
        let directed = (2.0 * random - 1.0) * direction;
        let sign = if directed > 0.0 {
            1.0
        } else if directed < 0.0 {
            -1.0
        } else {
            0.0
        };
        sign * (minimum + directed.abs() * (maximum - minimum))
    }

    #[test]
    fn box_random_uses_one_signed_random_sample_per_axis() {
        let source = generic_particle_vertex_source();
        assert!(source.contains("vec3 box_direction = (2.0 * box_direction_random - 1.0)"));
        assert!(source.contains("+ abs(box_direction)"));
        assert!(source.contains("+ sign(box_direction) * box_distance"));
        assert!(!source.contains("box_sign_random"));
    }

    #[test]
    fn box_random_applies_direction_before_distance_range() {
        assert_eq!(native_box_offset(0.75, 0.0, 10.0, 30.0), 0.0);
        assert_eq!(native_box_offset(0.75, 1.0, 0.0, 20.0), 10.0);
        assert_eq!(native_box_offset(0.25, 1.0, 10.0, 30.0), -20.0);
        assert_eq!(native_box_offset(0.75, 2.0, 10.0, 30.0), 30.0);
    }
}
