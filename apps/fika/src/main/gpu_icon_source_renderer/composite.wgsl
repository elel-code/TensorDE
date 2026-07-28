struct Params {
    rect: vec4<f32>,
    canvas: vec2<f32>,
    radius: f32,
    mode: f32,
    color: vec4<f32>,
    angle: f32,
    inset: f32,
    blur: f32,
    opacity: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0)
    );
    let uv = corners[index];
    let center = params.rect.xy + params.rect.zw * 0.5;
    let local = (uv - vec2<f32>(0.5)) * params.rect.zw;
    let cosine = cos(params.angle);
    let sine = sin(params.angle);
    let pixel = center + vec2<f32>(
        local.x * cosine - local.y * sine,
        local.x * sine + local.y * cosine
    );
    var out: VertexOutput;
    out.position = vec4<f32>(
        pixel.x / params.canvas.x * 2.0 - 1.0,
        1.0 - pixel.y / params.canvas.y * 2.0,
        0.0,
        1.0
    );
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if params.mode > 1.5 {
        let content_size = max(params.rect.zw - vec2<f32>(params.inset * 2.0), vec2<f32>(1.0));
        let source_uv = (in.uv * params.rect.zw - vec2<f32>(params.inset)) / content_size;
        let dimensions = vec2<f32>(textureDimensions(source_texture));
        var alpha = 0.0;
        for (var y = -2; y <= 2; y = y + 1) {
            for (var x = -2; x <= 2; x = x + 1) {
                let offset = vec2<f32>(f32(x), f32(y)) * params.blur / dimensions;
                let sample_uv = source_uv + offset;
                if all(sample_uv >= vec2<f32>(0.0)) && all(sample_uv <= vec2<f32>(1.0)) {
                    alpha += textureSampleLevel(source_texture, source_sampler, sample_uv, 0.0).a;
                }
            }
        }
        alpha = alpha / 25.0 * params.opacity;
        return vec4<f32>(params.color.rgb * alpha, alpha);
    }
    if params.mode > 0.5 {
        let half_size = params.rect.zw * 0.5;
        let point = abs((in.uv - vec2<f32>(0.5)) * params.rect.zw);
        let radius = min(params.radius, min(half_size.x, half_size.y));
        let delta = max(point - (half_size - vec2<f32>(radius)), vec2<f32>(0.0));
        if length(delta) > radius {
            discard;
        }
    }
    return textureSample(source_texture, source_sampler, in.uv) * params.color;
}
