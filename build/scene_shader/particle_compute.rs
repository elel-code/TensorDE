//! Compute shader source for the retained particle indirect-draw contract.

pub(crate) fn particle_compute_source() -> String {
    r#"#version 450
layout(local_size_x = 64) in;

struct ParticleEmitterState {
    vec4 timeScaleRateStartCapacity;
    vec4 lifetimeMinMaxProfileFlags;
    vec4 emitterOrigin;
    vec4 emitterDirections;
    vec4 velocityMin;
    vec4 velocityMax;
    vec4 gravity;
    vec4 sizeMinMaxFade;
};

struct DrawIndirect {
    uint vertexCount;
    uint instanceCount;
    uint firstVertex;
    uint firstInstance;
};

layout(set = 0, binding = 0, std430) readonly buffer ParticleStates {
    ParticleEmitterState states[];
};
layout(set = 0, binding = 1, std430) buffer ParticleIndirect {
    DrawIndirect draws[];
};
layout(set = 0, binding = 2, std430) readonly buffer ParticleFrame {
    float sceneTime;
} frameState;

void main() {
    uint emitter = gl_GlobalInvocationID.x;
    if (emitter >= states.length() || emitter >= draws.length()) {
        return;
    }
    float now = frameState.sceneTime * states[emitter].timeScaleRateStartCapacity.x;
    float start = states[emitter].timeScaleRateStartCapacity.z;
    float rate = max(states[emitter].timeScaleRateStartCapacity.y, 0.0);
    uint capacity = uint(max(states[emitter].timeScaleRateStartCapacity.w, 0.0));
    uint spawned = now < start ? 0u : uint(floor((now - start) * rate)) + 1u;
    draws[emitter].instanceCount = min(spawned, capacity);
}
"#
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_contract_updates_indirect_instance_count() {
        let source = particle_compute_source();
        assert!(source.contains("DrawIndirect"));
        assert!(source.contains("draws[emitter].instanceCount"));
        assert!(source.contains("binding = 2"));
        assert!(source.contains("frameState.sceneTime"));
        assert!(source.contains("local_size_x = 64"));
    }
}
