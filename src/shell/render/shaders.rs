pub(crate) const QUAD_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

pub(crate) const ICON_TEXTURE_SHADER: &str = r#"
@group(0) @binding(0)
var icon_texture: texture_2d<f32>;

@group(0) @binding(1)
var icon_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) rounding_bounds: vec4<f32>,
    @location(3) radius_alpha: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) rounding_bounds: vec4<f32>,
    @location(2) @interpolate(flat) radius_alpha: vec2<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    out.rounding_bounds = input.rounding_bounds;
    out.radius_alpha = input.radius_alpha;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    var color = textureSample(icon_texture, icon_sampler, input.uv);
    let bounds = input.rounding_bounds;
    if input.radius_alpha.x > 0.0 && bounds.z > bounds.x && bounds.w > bounds.y {
        let extent = bounds.zw - bounds.xy;
        let radius = min(extent.x, extent.y) * input.radius_alpha.x;
        let center = clamp(input.uv, bounds.xy + radius, bounds.zw - radius);
        let distance = length(input.uv - center) - radius;
        let antialias = max(fwidth(distance) * 0.5, 0.00001);
        color.a *= 1.0 - smoothstep(-antialias, antialias, distance);
    }
    color.a *= input.radius_alpha.y;
    return color;
}
"#;

pub(crate) const RETAINED_SCENE_SHADER: &str = r#"
@group(0) @binding(0)
var retained_scene: texture_2d<f32>;

@group(0) @binding(1)
var retained_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(retained_scene, retained_sampler, input.uv) * input.color;
}
"#;

pub(crate) const TEXT_SHADER: &str = r#"
@group(0) @binding(0)
var text_atlas: texture_2d<f32>;

@group(0) @binding(1)
var text_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(text_atlas, text_sampler, input.uv).r;
    return vec4<f32>(input.color.rgb, input.color.a * mask);
}
"#;
