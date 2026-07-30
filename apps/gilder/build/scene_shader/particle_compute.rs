//! Native Slang compute source for the retained particle indirect-draw contract.

pub(crate) fn particle_compute_source() -> &'static str {
    r#"struct ParticleEmitterState
{
    float4 timeScaleRateStartCapacity;
    float4 lifetimeMinMaxProfileFlags;
    float4 emitterOrigin;
    float4 emitterDirections;
    float4 velocityMin;
    float4 velocityMax;
    float4 gravity;
    float4 sizeMinMaxFade;
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

[[shader("compute")]]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint stateCount;
    uint stateStride;
    states.GetDimensions(stateCount, stateStride);
    uint drawCount;
    uint drawStride;
    draws.GetDimensions(drawCount, drawStride);
    uint emitter = dispatchThreadId.x;
    if (emitter >= stateCount || emitter >= drawCount)
    {
        return;
    }
    float now = frameState[0] * states[emitter].timeScaleRateStartCapacity.x;
    float start = states[emitter].timeScaleRateStartCapacity.z;
    float rate = max(states[emitter].timeScaleRateStartCapacity.y, 0.0);
    uint capacity = uint(max(states[emitter].timeScaleRateStartCapacity.w, 0.0));
    uint spawned = now < start ? 0u : uint(floor((now - start) * rate)) + 1u;
    draws[emitter].instanceCount = min(spawned, capacity);
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_contract_updates_indirect_instance_count() {
        let source = particle_compute_source();
        assert!(source.contains("DrawIndirect"));
        assert!(source.contains("draws[emitter].instanceCount"));
        assert!(source.contains("StructuredBuffer<float> frameState : register(t2)"));
        assert!(source.contains("frameState[0]"));
        assert!(source.contains("[numthreads(64, 1, 1)]"));
        assert!(!source.contains("#version"));
        assert!(!source.contains("layout("));
    }
}
