//! Slang compute source for retained particle simulation and indirect draws.

fn particle_noise_source() -> &'static str {
    let vertex = include_str!("../../shaders/scene/genericparticle.vert.slang");
    let start = vertex
        .find("uint gradient_noise_permutation_word")
        .expect("generic particle gradient-noise start");
    let end = vertex
        .find("vec2 billboard_corner")
        .expect("generic particle gradient-noise end");
    &vertex[start..end]
}

pub(crate) fn particle_compute_source() -> String {
    [
        r#"#define vec3 float3
#define ivec3 int3
#define fract frac
#define mix lerp

struct ParticleEmitterState
{
    float4 timeScaleRateStartCapacity;
    float4 lifetimeMinMaxProfileFlags;
    float4 emitterOrigin;
    float4 emitterDirections;
    float4 distanceMin;
    float4 distanceMax;
    float4 velocityMin;
    float4 velocityMax;
    float4 gravity;
    float4 sizeMinMaxFade;
    float4 turbulentVelocity;
    float4 turbulentVelocityPhaseTime;
    float4 turbulentVelocityRight;
    float4 turbulentVelocityForward;
    float4 turbulenceMaskScale;
    float4 turbulenceSpeedPhaseTime;
    float4 colorMinAlpha;
    float4 colorMaxAlpha;
    float4 rotationMinMax;
};

struct ParticleState
{
    float4 positionBirth;
    float4 velocityStable;
    float4 lifetimeSizeAlphaRotation;
    float4 colorLastTime;
};

struct DrawIndirect
{
    uint vertexCount;
    uint instanceCount;
    uint firstVertex;
    uint firstInstance;
};

StructuredBuffer<ParticleEmitterState> states : register(t0);
RWStructuredBuffer<DrawIndirect> draws : register(u1);
StructuredBuffer<float> frameState : register(t2);
RWStructuredBuffer<ParticleState> particles : register(u3);
RWStructuredBuffer<uint> randomStates : register(u4);

static const uint MT_WORD_COUNT = 624u;
static const uint RANDOM_STATE_WORD_COUNT = 628u;

void mt_seed(uint base, uint seed)
{
    randomStates[base] = seed;
    for (uint index = 1u; index < MT_WORD_COUNT; ++index)
    {
        uint previous = randomStates[base + index - 1u];
        randomStates[base + index] = 1812433253u
            * (previous ^ (previous >> 30u)) + index;
    }
    randomStates[base + MT_WORD_COUNT] = MT_WORD_COUNT;
}

uint mt_next(uint base)
{
    uint index = randomStates[base + MT_WORD_COUNT];
    if (index >= MT_WORD_COUNT)
    {
        for (uint word = 0u; word < MT_WORD_COUNT; ++word)
        {
            uint next = (word + 1u) % MT_WORD_COUNT;
            uint mixed = (randomStates[base + word] & 0x80000000u)
                | (randomStates[base + next] & 0x7fffffffu);
            uint twisted = randomStates[base + ((word + 397u) % MT_WORD_COUNT)]
                ^ (mixed >> 1u);
            if ((mixed & 1u) != 0u)
            {
                twisted ^= 0x9908b0dfu;
            }
            randomStates[base + word] = twisted;
        }
        index = 0u;
    }
    uint value = randomStates[base + index];
    randomStates[base + MT_WORD_COUNT] = index + 1u;
    value ^= value >> 11u;
    value ^= (value << 7u) & 0x9d2c5680u;
    value ^= (value << 15u) & 0xefc60000u;
    value ^= value >> 18u;
    return value;
}

float mt_random01(uint base)
{
    return float(mt_next(base) >> 8u) * (1.0 / 16777216.0);
}

"#,
        particle_noise_source(),
        r#"
vec3 rotate_axis_angle(vec3 direction, vec3 axis, float angle)
{
    float axisLength = length(axis);
    if (axisLength <= 0.000001)
    {
        return direction;
    }
    axis /= axisLength;
    float cosine = cos(angle);
    float sine = sin(angle);
    return direction * cosine
        + cross(axis, direction) * sine
        + axis * dot(axis, direction) * (1.0 - cosine);
}

uint initializer_kind(ParticleEmitterState emitterState, uint index)
{
    uint packed = index < 4u
        ? uint(emitterState.velocityMin.w + 0.5)
        : uint(emitterState.velocityMax.w + 0.5);
    return (packed >> ((index & 3u) * 4u)) & 15u;
}

vec3 safe_normalize(vec3 value)
{
    float squaredLength = dot(value, value);
    return squaredLength > 0.0 ? value * rsqrt(squaredLength) : vec3(0.0);
}

void apply_particle_step(
    inout ParticleState particle,
    ParticleEmitterState emitterState,
    float sampleTime,
    float movementDelta,
    float turbulenceDelta)
{
    particle.positionBirth.xyz += particle.velocityStable.xyz * movementDelta;
    particle.velocityStable.xyz += emitterState.gravity.xyz * movementDelta;
    uint moduleMask = uint(max(emitterState.distanceMin.w, 0.0));
    if ((moduleMask & (1u << 10u)) != 0u)
    {
        float stableRandom = particle.velocityStable.w;
        float speed = mix(
            emitterState.turbulenceSpeedPhaseTime.x,
            emitterState.turbulenceSpeedPhaseTime.y,
            stableRandom);
        float sampleBase = sampleTime * emitterState.turbulenceSpeedPhaseTime.w
            + stableRandom * emitterState.turbulenceSpeedPhaseTime.z;
        vec3 samplePosition = (particle.positionBirth.xyz + vec3(sampleBase))
            * emitterState.turbulenceMaskScale.w;
        float noise = simplex_noise_3d(samplePosition);
        particle.velocityStable.xyz += noise * speed * turbulenceDelta
            * emitterState.turbulenceMaskScale.xyz;
    }
}

void integrate_particle_prewarm(
    inout ParticleState particle,
    ParticleEmitterState emitterState,
    float targetTime,
    float turbulenceDeltaScale)
{
    float time = particle.colorLastTime.w;
    float remaining = max(targetTime - time, 0.0);
    for (uint stepIndex = 0u; stepIndex < 4096u && remaining > 0.0; ++stepIndex)
    {
        float step = min(remaining, 1.0 / 60.0);
        apply_particle_step(
            particle,
            emitterState,
            time + step,
            step,
            step * turbulenceDeltaScale);
        time += step;
        remaining -= step;
    }
    if (remaining > 0.0)
    {
        apply_particle_step(
            particle,
            emitterState,
            targetTime,
            remaining,
            remaining * turbulenceDeltaScale);
        time += remaining;
    }
    particle.colorLastTime.w = time;
}

void integrate_particle_live(
    inout ParticleState particle,
    ParticleEmitterState emitterState,
    float targetTime,
    float frameDelta,
    float turbulenceDeltaScale)
{
    float movementDelta = min(
        max(frameDelta, 0.0),
        max(targetTime - particle.colorLastTime.w, 0.0));
    if (movementDelta > 0.0)
    {
        apply_particle_step(
            particle,
            emitterState,
            targetTime,
            movementDelta,
            movementDelta * turbulenceDeltaScale);
    }
    particle.colorLastTime.w = targetTime;
}

void initialize_particle(
    uint randomBase,
    uint stateIndex,
    uint emission,
    ParticleEmitterState emitterState,
    float birthTime,
    float now,
    bool prewarm,
    float turbulenceDeltaScale)
{
    vec3 position = emitterState.emitterOrigin.xyz;
    vec3 velocity = vec3(0.0);
    bool boxEmitter = emitterState.emitterDirections.w >= 1.5;
    if (boxEmitter)
    {
        vec3 directed = vec3(
            2.0 * mt_random01(randomBase) - 1.0,
            2.0 * mt_random01(randomBase) - 1.0,
            2.0 * mt_random01(randomBase) - 1.0)
            * emitterState.emitterDirections.xyz;
        vec3 distance = emitterState.distanceMin.xyz
            + abs(directed) * (emitterState.distanceMax.xyz - emitterState.distanceMin.xyz);
        position += sign(directed) * distance;
    }
    else
    {
        float azimuth = mt_random01(randomBase) * 6.28318530718;
        float axial = 2.0 * mt_random01(randomBase) - 1.0;
        float radius = pow(mt_random01(randomBase), 1.0 / 3.0);
        float radial = sqrt(max(1.0 - axial * axial, 0.0));
        vec3 direction = vec3(cos(azimuth) * radial, sin(azimuth) * radial, axial)
            * emitterState.emitterDirections.xyz * radius;
        float directionLength = length(direction);
        vec3 offset = safe_normalize(direction)
            * (emitterState.distanceMin.xyz
                + directionLength
                    * (emitterState.distanceMax.xyz - emitterState.distanceMin.xyz));
        position += offset;
        vec3 velocityDirection = offset;
        if (dot(offset, offset) < 0.0001)
        {
            velocityDirection = vec3(
                2.0 * mt_random01(randomBase) - 1.0,
                2.0 * mt_random01(randomBase) - 1.0,
                2.0 * mt_random01(randomBase) - 1.0)
                * emitterState.emitterDirections.xyz;
        }
        float speed = mix(
            emitterState.gravity.w,
            emitterState.turbulentVelocityPhaseTime.w,
            mt_random01(randomBase));
        velocity = safe_normalize(velocityDirection) * speed;
    }

    float stableRandom = mt_random01(randomBase);
    float lifetime = 1.0;
    float size = 0.5;
    float alpha = 1.0;
    float rotation = 0.0;
    vec3 color = vec3(1.0);
    uint initializerCount = uint(emitterState.distanceMax.w + 0.5);
    for (uint initializer = 0u; initializer < initializerCount; ++initializer)
    {
        uint kind = initializer_kind(emitterState, initializer);
        if (kind == 1u)
        {
            lifetime = mix(
                emitterState.lifetimeMinMaxProfileFlags.x,
                emitterState.lifetimeMinMaxProfileFlags.y,
                mt_random01(randomBase));
        }
        else if (kind == 2u)
        {
            size = mix(
                emitterState.sizeMinMaxFade.x,
                emitterState.sizeMinMaxFade.y,
                mt_random01(randomBase));
        }
        else if (kind == 3u)
        {
            velocity += mix(
                emitterState.velocityMin.xyz,
                emitterState.velocityMax.xyz,
                vec3(
                    mt_random01(randomBase),
                    mt_random01(randomBase),
                    mt_random01(randomBase)));
        }
        else if (kind == 4u)
        {
            float random = mt_random01(randomBase);
            color = mix(emitterState.colorMinAlpha.xyz, emitterState.colorMaxAlpha.xyz, random);
        }
        else if (kind == 5u)
        {
            alpha = mix(
                emitterState.colorMinAlpha.w,
                emitterState.colorMaxAlpha.w,
                mt_random01(randomBase));
        }
        else if (kind == 6u)
        {
            rotation = mix(
                emitterState.rotationMinMax.x,
                emitterState.rotationMinMax.y,
                mt_random01(randomBase));
        }
        else if (kind == 7u)
        {
            float phase = mix(
                emitterState.turbulentVelocityPhaseTime.x,
                emitterState.turbulentVelocityPhaseTime.y,
                mt_random01(randomBase));
            float angle = gradient_noise_1d(
                (birthTime + phase) * emitterState.turbulentVelocityPhaseTime.z)
                * 3.14159265359 * emitterState.turbulentVelocity.y
                + emitterState.turbulentVelocity.x;
            vec3 direction = rotate_axis_angle(
                emitterState.turbulentVelocityForward.xyz,
                emitterState.turbulentVelocityRight.xyz,
                angle);
            float speed = mix(
                emitterState.turbulentVelocity.z,
                emitterState.turbulentVelocity.w,
                mt_random01(randomBase));
            velocity += direction * speed;
        }
    }

    ParticleState particle;
    particle.positionBirth = float4(position, birthTime);
    particle.velocityStable = float4(velocity, stableRandom);
    particle.lifetimeSizeAlphaRotation = float4(lifetime, size, alpha, rotation);
    particle.colorLastTime = float4(color, birthTime);
    if (prewarm)
    {
        integrate_particle_prewarm(
            particle,
            emitterState,
            now,
            turbulenceDeltaScale);
    }
    else
    {
        integrate_particle_live(
            particle,
            emitterState,
            now,
            now - birthTime,
            turbulenceDeltaScale);
    }
    particles[stateIndex] = particle;
}

[[shader("compute")]]
[numthreads(64, 1, 1)]
void main(uint3 groupId : SV_GroupID, uint3 groupThreadId : SV_GroupThreadID)
{
    uint stateCount;
    uint stateStride;
    states.GetDimensions(stateCount, stateStride);
    uint drawCount;
    uint drawStride;
    draws.GetDimensions(drawCount, drawStride);
    uint emitter = groupId.x;
    if (emitter >= stateCount || emitter >= drawCount)
    {
        return;
    }

    ParticleEmitterState emitterState = states[emitter];
    uint capacity = uint(max(emitterState.timeScaleRateStartCapacity.w, 0.0));
    float now = frameState[0] * emitterState.timeScaleRateStartCapacity.x;
    float frameDelta = frameState[1] * emitterState.timeScaleRateStartCapacity.x;
    float transformScale = frameState[2u + emitter];
    float turbulenceDeltaScale = pow(
        min(transformScale > 0.0 ? 0.025 / transformScale : 1.0, 1.0),
        0.7);
    float start = emitterState.timeScaleRateStartCapacity.z;
    float rate = max(emitterState.timeScaleRateStartCapacity.y, 0.0);
    uint spawned = uint(floor(max(now + start, 0.0) * rate));
    uint stateOffset = uint(max(emitterState.emitterOrigin.w, 0.0));
    uint randomBase = emitter * RANDOM_STATE_WORD_COUNT;
    if (groupThreadId.x == 0u)
    {
        draws[emitter].instanceCount = min(spawned, capacity);
        float previousNow = asfloat(randomStates[randomBase + MT_WORD_COUNT + 2u]);
        uint emitted = randomStates[randomBase + MT_WORD_COUNT + 1u];
        bool coldStart = previousNow != previousNow;
        if (previousNow == previousNow && now < previousNow)
        {
            mt_seed(randomBase, randomStates[randomBase + MT_WORD_COUNT + 3u]);
            emitted = 0u;
            coldStart = true;
        }
        for (uint emission = emitted; emission < spawned && capacity != 0u; ++emission)
        {
            uint localParticle = emission % capacity;
            float birthTime = float(emission + 1u) / max(rate, 0.0001) - start;
            initialize_particle(
                randomBase,
                stateOffset + localParticle,
                emission,
                emitterState,
                birthTime,
                now,
                coldStart,
                turbulenceDeltaScale);
        }
        randomStates[randomBase + MT_WORD_COUNT + 1u] = spawned;
        randomStates[randomBase + MT_WORD_COUNT + 2u] = asuint(now);
    }

    GroupMemoryBarrierWithGroupSync();
    bool retained = emitterState.lifetimeMinMaxProfileFlags.z >= 0.5;
    if (!retained)
    {
        return;
    }
    for (uint localParticle = groupThreadId.x;
        localParticle < min(capacity, spawned);
        localParticle += 64u)
    {
        uint stateIndex = stateOffset + localParticle;
        ParticleState particle = particles[stateIndex];
        if (particle.colorLastTime.w < now)
        {
            integrate_particle_live(
                particle,
                emitterState,
                now,
                frameDelta,
                turbulenceDeltaScale);
            particles[stateIndex] = particle;
        }
    }
}
"#,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_contract_updates_retained_particles_before_indirect_draws() {
        let source = particle_compute_source();
        assert!(source.contains("RWStructuredBuffer<ParticleState> particles : register(u3)"));
        assert!(source.contains("RWStructuredBuffer<uint> randomStates : register(u4)"));
        assert!(source.contains("draws[emitter].instanceCount"));
        assert!(source.contains("void mt_seed(uint base, uint seed)"));
        assert!(source.contains("uint mt_next(uint base)"));
        assert!(source.contains("initialize_particle("));
        assert!(source.contains("* emitterState.emitterDirections.xyz * radius"));
        assert!(source.contains("float(emission + 1u) / max(rate, 0.0001) - start"));
        assert!(source.contains("void integrate_particle_prewarm("));
        assert!(source.contains("void integrate_particle_live("));
        assert!(source.contains("float frameDelta = frameState[1]"));
        assert!(source.contains("float transformScale = frameState[2u + emitter]"));
        assert!(source.contains("0.025 / transformScale"));
        assert!(source.contains("movementDelta * turbulenceDeltaScale"));
        assert!(source.contains("sampleTime * emitterState.turbulenceSpeedPhaseTime.w"));
        assert!(
            source.contains(
                "particle.positionBirth.xyz += particle.velocityStable.xyz * movementDelta"
            )
        );
        assert!(source.contains("particle.velocityStable.xyz += noise * speed * turbulenceDelta"));
        assert!(source.contains("simplex_noise_3d(samplePosition)"));
        assert!(source.contains("GroupMemoryBarrierWithGroupSync"));
        assert!(source.contains("[numthreads(64, 1, 1)]"));
        assert!(!source.contains("#version"));
        assert!(!source.contains("layout("));
    }
}
