#define vec2 float2
#define vec3 float3
#define vec4 float4
#define ivec2 int2
#define ivec3 int3
#define ivec4 int4
#define uvec2 uint2
#define uvec3 uint3
#define uvec4 uint4
#define bvec2 bool2
#define bvec3 bool3
#define bvec4 bool4
#define mat2 float2x2
#define mat3 float3x3
#define mat4 float4x4
#define fract frac
#define mix lerp
#define inversesqrt rsqrt
#define roundEven round
#define texture2D(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture3D(S, UVW) S ## _texture.Sample(S ## _sampler, UVW)
#define texture(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture2DLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define textureLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define texelFetch(S, COORD, LOD) S ## _texture.Load(int3(COORD, LOD))
#define textureSize(S, LOD) tensorWallpaperTextureSize_ ## S(LOD)
#define greaterThan(A, B) ((A) > (B))
#define greaterThanEqual(A, B) ((A) >= (B))
#define lessThan(A, B) ((A) < (B))
#define lessThanEqual(A, B) ((A) <= (B))
#define equal(A, B) ((A) == (B))
#define notEqual(A, B) ((A) != (B))
#define mod(A, B) ((A) - (B) * floor((A) / (B)))

struct SceneDrawTransform
{
    vec4 g_ModelViewProjectionMatrix[4];
};
cbuffer SceneDrawTransform_Buffer : register(b2)
{
    SceneDrawTransform g_Draw;
}
struct ParticleMaterial
{
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
    vec4 g_TurbulenceOperator;
    vec4 g_TurbulencePhase;
    vec4 g_SizeChangeRenderer;
    vec4 g_VortexAxisDistanceInner;
    vec4 g_VortexDistanceSpeedTrail;
    vec4 g_TrailRendererFlags;
    vec4 g_TurbulentVelocityRightOperatorSpeedMin;
    vec4 g_TurbulentVelocityForwardOperatorSpeedMax;
    vec4 g_TurbulenceOperatorTimeState;
    vec4 g_TrailEyeMax;
    vec4 g_TextureSequence;
    vec4 g_TextureSequenceOrigin;
};
cbuffer ParticleMaterial_Buffer : register(b3)
{
    ParticleMaterial g_Particle;
}
struct ParticleSimulationState
{
    vec4 positionBirth;
    vec4 velocityStable;
    vec4 lifetimeSizeAlphaRotation;
    vec4 colorLastTime;
};
StructuredBuffer<ParticleSimulationState> g_ParticleStates : register(t5);

static uint gl_VertexIndex;
static uint gl_InstanceIndex;
static vec2 v_TexCoord;
static vec2 v_TexCoordNext;
static float v_VertexAlpha;
static vec3 v_ParticleColor;
static float v_TextureDecodeMode;
static float v_TextureSequenceBlend;
static float4 tensorWallpaperPosition;
struct TensorWallpaperVertexInput
{
    uint tensorWallpaperVertexIndex : SV_VertexID;
    uint tensorWallpaperInstanceIndex : SV_InstanceID;
};
struct TensorWallpaperVertexOutput
{
    float4 position : SV_Position;
    [[vk::location(0)]] vec2 v_TexCoord : TEXCOORD0;
    [[vk::location(1)]] float v_VertexAlpha : TEXCOORD1;
    [[vk::location(2)]] vec3 v_ParticleColor : TEXCOORD2;
    nointerpolation [[vk::location(3)]] float v_TextureDecodeMode : TEXCOORD3;
    [[vk::location(4)]] vec2 v_TexCoordNext : TEXCOORD4;
    nointerpolation [[vk::location(5)]] float v_TextureSequenceBlend : TEXCOORD5;
};

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
uint gradient_noise_permutation_word(uint index) {
    switch (index) {
        case 0u: return 0x5b89a097u;
        case 1u: return 0x0d830f5au;
        case 2u: return 0x35605fc9u;
        case 3u: return 0xe107e9c2u;
        case 4u: return 0x1e67248cu;
        case 5u: return 0x63088e45u;
        case 6u: return 0x0a15f025u;
        case 7u: return 0x9406be17u;
        case 8u: return 0x4bea78f7u;
        case 9u: return 0x3ec51a00u;
        case 10u: return 0xcbdbfc5eu;
        case 11u: return 0x200b2375u;
        case 12u: return 0x5821b139u;
        case 13u: return 0x573895edu;
        case 14u: return 0x887d14aeu;
        case 15u: return 0xaf44a8abu;
        case 16u: return 0x8647a54au;
        case 17u: return 0xa61b308bu;
        case 18u: return 0xe79e924du;
        case 19u: return 0x7ae56f53u;
        case 20u: return 0xe685d33cu;
        case 21u: return 0x295c69dcu;
        case 22u: return 0x28f52e37u;
        case 23u: return 0x368f66f4u;
        case 24u: return 0xa13f1941u;
        case 25u: return 0x4950d801u;
        case 26u: return 0xbb844cd1u;
        case 27u: return 0xa91259d0u;
        case 28u: return 0x8287c4c8u;
        case 29u: return 0x569fbc74u;
        case 30u: return 0xc66d64a4u;
        case 31u: return 0x4003baadu;
        case 32u: return 0xfae2d934u;
        case 33u: return 0xca057b7cu;
        case 34u: return 0x7e769326u;
        case 35u: return 0xd45552ffu;
        case 36u: return 0xe33bcecfu;
        case 37u: return 0x113a102fu;
        case 38u: return 0x2a1cbdb6u;
        case 39u: return 0xd5aab7dfu;
        case 40u: return 0x0298f877u;
        case 41u: return 0x46a39a2cu;
        case 42u: return 0x9b6599ddu;
        case 43u: return 0x09ac2ba7u;
        case 44u: return 0xfd271681u;
        case 45u: return 0x6e6c6213u;
        case 46u: return 0xe8e0714fu;
        case 47u: return 0x6870b9b2u;
        case 48u: return 0xe461f6dau;
        case 49u: return 0xc1f222fbu;
        case 50u: return 0x0c90d2eeu;
        case 51u: return 0xf1a2b3bfu;
        case 52u: return 0xeb913351u;
        case 53u: return 0x6bef0ef9u;
        case 54u: return 0x1fd6c031u;
        case 55u: return 0x9d6ac7b5u;
        case 56u: return 0xb0cc54b8u;
        case 57u: return 0x2d327973u;
        case 58u: return 0xfe96047fu;
        case 59u: return 0x5dcdec8au;
        case 60u: return 0x1d4372deu;
        case 61u: return 0x8df34818u;
        case 62u: return 0x424ec380u;
        default: return 0xb49c3dd7u;
    }
}
uint gradient_noise_permutation(uint index) {
    uint packed = gradient_noise_permutation_word(index >> 2u);
    return (packed >> ((index & 3u) * 8u)) & 255u;
}
float gradient_noise_gradient(uint permutation, float distance) {
    float gradient = float(1u + (permutation & 7u));
    if ((permutation & 8u) != 0u) {
        gradient = -gradient;
    }
    float falloff = 1.0 - distance * distance;
    falloff *= falloff;
    falloff *= falloff;
    return gradient * distance * falloff;
}
float gradient_noise_1d(float value) {
    int lattice = int(floor(value));
    float distance = value - float(lattice);
    uint leftIndex = uint(lattice & 255);
    uint rightIndex = uint((lattice + 1) & 255);
    float left = gradient_noise_gradient(gradient_noise_permutation(leftIndex), distance);
    float right = gradient_noise_gradient(
        gradient_noise_permutation(rightIndex),
        distance - 1.0);
    return (left + right) * 0.395;
}
vec3 simplex_noise_gradient(uint index) {
    switch (index) {
        case 0u: return vec3(1.0, 1.0, 0.0);
        case 1u: return vec3(-1.0, 1.0, 0.0);
        case 2u: return vec3(1.0, -1.0, 0.0);
        case 3u: return vec3(-1.0, -1.0, 0.0);
        case 4u: return vec3(1.0, 0.0, 1.0);
        case 5u: return vec3(-1.0, 0.0, 1.0);
        case 6u: return vec3(1.0, 0.0, -1.0);
        case 7u: return vec3(-1.0, 0.0, -1.0);
        case 8u: return vec3(0.0, 1.0, 1.0);
        case 9u: return vec3(0.0, -1.0, 1.0);
        case 10u: return vec3(0.0, 1.0, -1.0);
        default: return vec3(0.0, -1.0, -1.0);
    }
}
uint simplex_noise_gradient_index(ivec3 cell) {
    uint z = gradient_noise_permutation(uint(cell.z & 255));
    uint y = gradient_noise_permutation(uint((cell.y + int(z)) & 255));
    return gradient_noise_permutation(uint((cell.x + int(y)) & 255)) % 12u;
}
float simplex_noise_corner(ivec3 cell, vec3 distance) {
    float attenuation = 0.6 - dot(distance, distance);
    if (attenuation <= 0.0) {
        return 0.0;
    }
    attenuation *= attenuation;
    vec3 gradient = simplex_noise_gradient(simplex_noise_gradient_index(cell));
    return attenuation * attenuation * dot(gradient, distance);
}
float simplex_noise_3d(vec3 position) {
    const float F3 = 0.3333333333333333;
    const float G3 = 0.1666666666666667;
    ivec3 cell = ivec3(floor(position + dot(position, vec3(F3))));
    vec3 distance0 = position - (vec3(cell) - dot(vec3(cell), vec3(G3)));
    ivec3 corner1;
    ivec3 corner2;
    if (distance0.x >= distance0.y) {
        if (distance0.y >= distance0.z) {
            corner1 = ivec3(1, 0, 0);
            corner2 = ivec3(1, 1, 0);
        } else if (distance0.x >= distance0.z) {
            corner1 = ivec3(1, 0, 0);
            corner2 = ivec3(1, 0, 1);
        } else {
            corner1 = ivec3(0, 0, 1);
            corner2 = ivec3(1, 0, 1);
        }
    } else if (distance0.y < distance0.z) {
        corner1 = ivec3(0, 0, 1);
        corner2 = ivec3(0, 1, 1);
    } else if (distance0.x < distance0.z) {
        corner1 = ivec3(0, 1, 0);
        corner2 = ivec3(0, 1, 1);
    } else {
        corner1 = ivec3(0, 1, 0);
        corner2 = ivec3(1, 1, 0);
    }
    vec3 distance1 = distance0 - vec3(corner1) + vec3(G3);
    vec3 distance2 = distance0 - vec3(corner2) + vec3(2.0 * G3);
    vec3 distance3 = distance0 - vec3(0.5);
    return 32.0 * (
        simplex_noise_corner(cell, distance0)
        + simplex_noise_corner(cell + corner1, distance1)
        + simplex_noise_corner(cell + corner2, distance2)
        + simplex_noise_corner(cell + ivec3(1), distance3));
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
struct ParticleTextureCoordinates {
    vec2 current;
    vec2 next;
    float blend;
};
vec2 particle_texture_frame_uv(vec2 localUv, float frame) {
    float frameWidth = g_Particle.g_TextureSequence.x;
    float frameHeight = g_Particle.g_TextureSequence.y;
    float rowStride = g_Particle.g_TextureSequence.w;
    vec2 origin = g_Particle.g_TextureSequenceOrigin.xy + vec2(
        fmod(frame, rowStride) * frameWidth,
        floor(frame / rowStride) * frameHeight);
    return origin + localUv * vec2(frameWidth, frameHeight);
}
ParticleTextureCoordinates particle_texture_coordinates(
    vec2 localUv,
    float lifetimeFraction,
    float stableRandom) {
    ParticleTextureCoordinates coordinates;
    float frameCount = g_Particle.g_TextureSequence.z;
    if (frameCount < 1.0) {
        coordinates.current = localUv;
        coordinates.next = localUv;
        coordinates.blend = 0.0;
        return coordinates;
    }
    bool randomFrame = g_Particle.g_TextureSequenceOrigin.z > 0.5;
    float sequencePhase = fract(
        lifetimeFraction * g_Particle.g_SequenceLifetimeSize.x);
    float framePosition = randomFrame
        ? stableRandom * frameCount
        : sequencePhase * frameCount;
    float currentFrame = min(frameCount - 1.0, floor(framePosition));
    float nextFrame = randomFrame
        ? currentFrame
        : min(frameCount - 1.0, currentFrame + 1.0);
    coordinates.current = particle_texture_frame_uv(localUv, currentFrame);
    coordinates.next = particle_texture_frame_uv(localUv, nextFrame);
    coordinates.blend = randomFrame ? 0.0 : fract(framePosition);
    return coordinates;
}
[[shader("vertex")]]
TensorWallpaperVertexOutput main(TensorWallpaperVertexInput input)
{
    gl_VertexIndex = input.tensorWallpaperVertexIndex;
    gl_InstanceIndex = input.tensorWallpaperInstanceIndex;
    TensorWallpaperVertexOutput output;

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
        + g_Particle.g_TimeStartRateCount.y - spawn_offset;
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
    float simulation = g_Particle.g_SimulationOscillation.x;
    bool retainedParticleState = g_Particle.g_TurbulenceOperatorTimeState.z >= 0.5;
    ParticleSimulationState retainedState;
    retainedState.positionBirth = vec4(0.0);
    retainedState.velocityStable = vec4(0.0);
    retainedState.lifetimeSizeAlphaRotation = vec4(0.0);
    retainedState.colorLastTime = vec4(0.0);
    if (retainedParticleState) {
        uint particleStateOffset = uint(max(
            g_Particle.g_TurbulenceOperatorTimeState.w,
            0.0));
        retainedState = g_ParticleStates[particleStateOffset + uint(gl_InstanceIndex)];
        lifetime = retainedState.lifetimeSizeAlphaRotation.x;
        age = max(g_Particle.g_TimeStartRateCount.x - retainedState.positionBirth.w, 0.0);
        elapsed = age;
    }
    float alive = elapsed >= 0.0
        && instance < active_slots
        && (capacity_saturated || age < lifetime) ? 1.0 : 0.0;
    if (retainedParticleState) {
        alive = age < lifetime ? 1.0 : 0.0;
    }
    if (alive < 0.5) {
        v_TexCoord = vec2(0.0);
        v_VertexAlpha = 0.0;
        v_ParticleColor = vec3(0.0);
        v_TextureDecodeMode = 0.0;
        tensorWallpaperPosition = vec4(2.0, 2.0, 0.0, 1.0);
        output.position = tensorWallpaperPosition;
        output.v_TexCoord = v_TexCoord;
        output.v_VertexAlpha = v_VertexAlpha;
        output.v_ParticleColor = v_ParticleColor;
        output.v_TextureDecodeMode = v_TextureDecodeMode;
        return output;
    }
    bool moduleSprite = simulation > 3.5;
    bool boxEmitter = moduleSprite
        ? g_Particle.g_BillboardTexture.w > 1.5
        : simulation > 1.5;
    uint moduleMask = uint(max(g_Particle.g_SizeOscillationScaleMaskZ.w, 0.0));
    bool positionOscillation = simulation > 2.5 && simulation < 3.5
        || (moduleSprite && (moduleMask & 256u) != 0u);
    bool sizeChange = moduleSprite && (moduleMask & 2048u) != 0u;
    bool vortex = moduleSprite && (moduleMask & 4096u) != 0u;
    bool spriteTrail = moduleSprite && g_Particle.g_SizeChangeRenderer.w > 1.5;
    bool turbulentVelocity = moduleSprite && (moduleMask & 512u) != 0u;
    // Each WE particle module advances its own stable random selection. Keep the
    // ambient-particle fields independent so position cannot bias size, color, or
    // alpha oscillation. The sphere path retains its established random stream.
    uint ambient_seed = uint(uint(max(g_Particle.g_SizeFadeSeed.w, 0.0) + 0.5)
        ^ (uint(gl_InstanceIndex) * 0x9e3779b9u)
        ^ (uint(max(cycle, 0.0)) * 0x85ebca6bu));
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
    if (!boxEmitter) {
        random0 = hash31(seed);
        random1 = hash31(seed + 101.0);
        random2 = hash31(seed + 211.0);
    }
    vec3 position;
    if (boxEmitter) {
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
        boxEmitter ? ambient_velocity_random : random1);
    if (turbulentVelocity) {
        float birthTime = spawn_offset + cycle * schedule_period;
        float velocityPhase = mix(
            g_Particle.g_TurbulencePhase.y,
            g_Particle.g_TurbulencePhase.z,
            boxEmitter ? ambient_turbulence_random.y : random2.y);
        float velocityWave = gradient_noise_1d(
            (birthTime + velocityPhase) * g_Particle.g_TurbulencePhase.w)
            * 3.14159265 * g_Particle.g_Turbulence.y
            + g_Particle.g_Turbulence.x;
        vec3 axis = g_Particle.g_TurbulentVelocityRightOperatorSpeedMin.xyz;
        axis /= length(axis);
        vec3 direction = g_Particle.g_TurbulentVelocityForwardOperatorSpeedMax.xyz;
        float velocityCosine = cos(velocityWave);
        float velocitySine = sin(velocityWave);
        vec3 turbulentDirection = direction * velocityCosine
            + cross(axis, direction) * velocitySine
            + axis * dot(axis, direction) * (1.0 - velocityCosine);
        float birthSpeed = mix(
            g_Particle.g_Turbulence.z,
            g_Particle.g_Turbulence.w,
            boxEmitter ? ambient_turbulence_random.x : random2.x);
        velocity += turbulentDirection * birthSpeed;
    }
    vec2 operatorTurbulentDirection = vec2(0.0);
    float operatorTurbulentSpeed = 0.0;
    float turbulenceScale = 0.0;
    vec2 turbulenceMask = vec2(0.0);
    if (retainedParticleState) {
        position = retainedState.positionBirth.xyz;
        velocity = retainedState.velocityStable.xyz;
    } else {
        position += velocity * age + g_Particle.g_Gravity.xyz * (0.5 * age * age);
        operatorTurbulentSpeed = mix(
            g_Particle.g_TurbulentVelocityRightOperatorSpeedMin.w,
            g_Particle.g_TurbulentVelocityForwardOperatorSpeedMax.w,
            boxEmitter ? ambient_turbulence_random.x : random2.x);
        float operatorTurbulentPhase = seed * 0.017
            + age * g_Particle.g_TurbulencePhase.x;
        operatorTurbulentDirection = vec2(
            sin(operatorTurbulentPhase),
            cos(operatorTurbulentPhase * 0.73));
        turbulenceScale = g_Particle.g_TurbulenceOperator.w;
        turbulenceMask = g_Particle.g_TurbulenceOperator.xy;
        position.xy += operatorTurbulentDirection * operatorTurbulentSpeed
            * turbulenceScale * turbulenceMask * age * 0.2;
    }

    if (vortex) {
        float radialDistance = length(position.xy - g_Particle.g_EmitterOrigin.xy);
        float distanceSpan = max(
            g_Particle.g_VortexDistanceSpeedTrail.x
                - g_Particle.g_VortexAxisDistanceInner.w,
            0.0001);
        float vortexMix = clamp(
            (radialDistance - g_Particle.g_VortexAxisDistanceInner.w) / distanceSpan,
            0.0,
            1.0);
        float angularSpeed = mix(
            g_Particle.g_VortexDistanceSpeedTrail.y,
            g_Particle.g_VortexDistanceSpeedTrail.z,
            vortexMix);
        float vortexAngle = angularSpeed * age;
        float2 radial = position.xy - g_Particle.g_EmitterOrigin.xy;
        position.xy = g_Particle.g_EmitterOrigin.xy + float2(
            cos(vortexAngle) * radial.x - sin(vortexAngle) * radial.y,
            sin(vortexAngle) * radial.x + cos(vortexAngle) * radial.y);
    }
    if (positionOscillation) {
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
        boxEmitter ? ambient_shape_random.x : random2.y);
    if (retainedParticleState) {
        size = retainedState.lifetimeSizeAlphaRotation.y;
    }
    if (simulation > 2.5 && simulation < 3.5) {
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
    if (sizeChange) {
        float changeStart = g_Particle.g_SizeChangeRenderer.x;
        float changeProgress = clamp(
            (age / max(lifetime, 0.0001) - changeStart)
                / max(1.0 - changeStart, 0.0001),
            0.0,
            1.0);
        size *= mix(
            g_Particle.g_SizeChangeRenderer.y,
            g_Particle.g_SizeChangeRenderer.z,
            changeProgress);
    }
    float rotation = mix(
        g_Particle.g_RotationColorMix.x,
        g_Particle.g_RotationColorMix.y,
        boxEmitter ? ambient_shape_random.y : random2.z);
    if (retainedParticleState) {
        rotation = retainedState.lifetimeSizeAlphaRotation.w;
    }
    float angular_velocity = mix(
        g_Particle.g_AngularVelocityMin.z,
        g_Particle.g_AngularVelocityMax.z,
        boxEmitter ? ambient_shape_random.z : random1.z);
    rotation += angular_velocity * age;
    vec2 authoredCorner = billboard_corner(gl_VertexIndex);
    vec4 local_position;
    if (spriteTrail) {
        vec3 trailVelocity = velocity;
        if (!retainedParticleState) {
            trailVelocity += vec3(operatorTurbulentDirection, 0.0)
                * operatorTurbulentSpeed * turbulenceScale
                * vec3(turbulenceMask, 0.0);
        }
        float trailSpeed = length(trailVelocity);
        vec3 tangent = trailVelocity / trailSpeed;
        vec3 eyeDirection = position - g_Particle.g_TrailEyeMax.xyz;
        vec3 normal = normalize(cross(eyeDirection, trailVelocity));
        float trailLength = clamp(
            trailSpeed * g_Particle.g_VortexDistanceSpeedTrail.w,
            g_Particle.g_TrailRendererFlags.x,
            g_Particle.g_TrailEyeMax.w);
        vec3 up = tangent * trailLength;
        local_position = vec4(
            position
                + normal * authoredCorner.x * size * 0.5
                + up * authoredCorner.y * size * 0.5
                    * g_Particle.g_BillboardTexture.x,
            1.0);
    } else {
        vec2 screen_corner = authoredCorner
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
        local_position = vec4(position.xy + corner, 0.0, 1.0);
    }
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
    float initializedAlpha = mix(
        g_Particle.g_BillboardTexture.y,
        g_Particle.g_BillboardTexture.z,
        boxEmitter ? ambient_shape_random.y : random2.z);
    if (retainedParticleState) {
        initializedAlpha = retainedState.lifetimeSizeAlphaRotation.z;
    }
    v_VertexAlpha = alive * initializedAlpha
        * min(fade_in, fade_out) * opacity_oscillation;
    v_ParticleColor = mix(
        g_Particle.g_ColorMin.xyz,
        g_Particle.g_ColorMax.xyz,
        boxEmitter ? ambient_color_random : random0);
    if (retainedParticleState) {
        v_ParticleColor = retainedState.colorLastTime.xyz;
    }
    v_TextureDecodeMode = g_Particle.g_OscillationScale.y;
    ParticleTextureCoordinates textureCoordinates = particle_texture_coordinates(
        billboard_uv(gl_VertexIndex),
        lifetime_fraction,
        hash11(seed + 313.0));
    v_TexCoord = textureCoordinates.current;
    v_TexCoordNext = textureCoordinates.next;
    v_TextureSequenceBlend = textureCoordinates.blend;
    // WE's orthographic scene particles simulate a three-component position, but the
    // authored Z is not Vulkan clip-space depth. Billboard ordering is owned by the
    // scene graph, so projecting the random sphere Z would clip almost every 2D leaf.
    tensorWallpaperPosition = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
    output.position = tensorWallpaperPosition;
    output.v_TexCoord = v_TexCoord;
    output.v_VertexAlpha = v_VertexAlpha;
    output.v_ParticleColor = v_ParticleColor;
    output.v_TextureDecodeMode = v_TextureDecodeMode;
    output.v_TexCoordNext = v_TexCoordNext;
    output.v_TextureSequenceBlend = v_TextureSequenceBlend;
    return output;
}
