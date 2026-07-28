use lyon::math::{Angle, point, vector};
use lyon::path::{Path as LyonPath, builder::SvgPathBuilder};
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor,
    LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor,
    VertexBuffers,
};

const GPU_SVG_SHADER: &str = r#"
struct In { @location(0) position: vec2<f32>, @location(1) color: vec4<f32> };
struct Out { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs_main(input: In) -> Out {
    var out: Out;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: Out) -> @location(0) vec4<f32> { return input.color; }
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSvgVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl GpuSvgVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy)]
struct SvgAffine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl SvgAffine {
    const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    fn then(self, rhs: Self) -> Self {
        Self {
            a: rhs.a * self.a + rhs.c * self.b,
            b: rhs.b * self.a + rhs.d * self.b,
            c: rhs.a * self.c + rhs.c * self.d,
            d: rhs.b * self.c + rhs.d * self.d,
            e: rhs.a * self.e + rhs.c * self.f + rhs.e,
            f: rhs.b * self.e + rhs.d * self.f + rhs.f,
        }
    }

    fn point(self, p: lyon::math::Point) -> lyon::math::Point {
        point(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    fn inverse(self) -> Option<Self> {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        Some(Self {
            a: self.d * inverse,
            b: -self.b * inverse,
            c: -self.c * inverse,
            d: self.a * inverse,
            e: (self.c * self.f - self.d * self.e) * inverse,
            f: (self.b * self.e - self.a * self.f) * inverse,
        })
    }
}

#[derive(Clone)]
struct SvgGradientStop {
    offset: f32,
    color: [f32; 4],
}

#[derive(Clone)]
enum SvgPaint {
    Solid([f32; 4]),
    Linear {
        start: [f32; 2],
        end: [f32; 2],
        object_bounding_box: bool,
        transform: SvgAffine,
        stops: Arc<[SvgGradientStop]>,
    },
    Radial {
        center: [f32; 2],
        radius: f32,
        object_bounding_box: bool,
        transform: SvgAffine,
        stops: Arc<[SvgGradientStop]>,
    },
}

impl SvgPaint {
    fn color_at(&self, position: lyon::math::Point, bounds: lyon::math::Box2D) -> [f32; 4] {
        match self {
            Self::Solid(color) => *color,
            Self::Linear {
                start,
                end,
                object_bounding_box,
                transform,
                stops,
            } => {
                let p = transform.inverse().unwrap_or(SvgAffine::IDENTITY).point(position);
                let (start, end) = if *object_bounding_box {
                    let size = bounds.size();
                    (
                        point(bounds.min.x + start[0] * size.width, bounds.min.y + start[1] * size.height),
                        point(bounds.min.x + end[0] * size.width, bounds.min.y + end[1] * size.height),
                    )
                } else {
                    (point(start[0], start[1]), point(end[0], end[1]))
                };
                let axis = end - start;
                let denominator = axis.square_length();
                let offset = if denominator > f32::EPSILON {
                    ((p - start).dot(axis) / denominator).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                sample_svg_gradient(stops, offset)
            }
            Self::Radial {
                center,
                radius,
                object_bounding_box,
                transform,
                stops,
            } => {
                let p = transform.inverse().unwrap_or(SvgAffine::IDENTITY).point(position);
                let (center, radius) = if *object_bounding_box {
                    let size = bounds.size();
                    (
                        point(bounds.min.x + center[0] * size.width, bounds.min.y + center[1] * size.height),
                        radius * size.width.max(size.height),
                    )
                } else {
                    (point(center[0], center[1]), *radius)
                };
                let offset = if radius > f32::EPSILON {
                    (p - center).length() / radius
                } else {
                    0.0
                };
                sample_svg_gradient(stops, offset.clamp(0.0, 1.0))
            }
        }
    }
}

#[derive(Clone)]
struct SvgPaintState {
    transform: SvgAffine,
    color: [f32; 4],
    fill: Option<SvgPaint>,
    stroke: Option<SvgPaint>,
    stroke_width: f32,
    stroke_cap: LineCap,
    stroke_join: LineJoin,
    fill_rule: FillRule,
    opacity: f32,
}

impl Default for SvgPaintState {
    fn default() -> Self {
        Self {
            transform: SvgAffine::IDENTITY,
            color: [0.0, 0.0, 0.0, 1.0],
            fill: Some(SvgPaint::Solid([0.0, 0.0, 0.0, 1.0])),
            stroke: None,
            stroke_width: 1.0,
            stroke_cap: LineCap::Butt,
            stroke_join: LineJoin::Miter,
            fill_rule: FillRule::NonZero,
            opacity: 1.0,
        }
    }
}

struct SvgVertexCtor {
    transform: SvgAffine,
    paint: SvgPaint,
    bounds: lyon::math::Box2D,
    opacity: f32,
}

impl SvgVertexCtor {
    fn make(&self, p: lyon::math::Point) -> GpuSvgVertex {
        let mut color = self.paint.color_at(p, self.bounds);
        color[3] *= self.opacity;
        color[0] *= color[3];
        color[1] *= color[3];
        color[2] *= color[3];
        let transformed = self.transform.point(p);
        GpuSvgVertex { position: [transformed.x, transformed.y], color }
    }
}

impl FillVertexConstructor<GpuSvgVertex> for SvgVertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> GpuSvgVertex { self.make(vertex.position()) }
}

impl StrokeVertexConstructor<GpuSvgVertex> for SvgVertexCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> GpuSvgVertex { self.make(vertex.position()) }
}

struct GpuSvgRenderer {
    rgba_pipeline: wgpu::RenderPipeline,
    bgra_pipeline: wgpu::RenderPipeline,
}

#[derive(Clone, Copy)]
struct SvgIntrinsicSize {
    width: f32,
    height: f32,
}

fn svg_intrinsic_size(bytes: &[u8]) -> Option<SvgIntrinsicSize> {
    let text = std::str::from_utf8(bytes).ok()?;
    let document = roxmltree::Document::parse(text).ok()?;
    let (_, _, width, height) = svg_view_box(document.root_element())?;
    (width > 0.0 && height > 0.0).then_some(SvgIntrinsicSize { width, height })
}

impl GpuSvgRenderer {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            rgba_pipeline: create_gpu_svg_pipeline(device, wgpu::TextureFormat::Rgba8Unorm),
            bgra_pipeline: create_gpu_svg_pipeline(device, wgpu::TextureFormat::Bgra8Unorm),
        }
    }

    fn render_bytes(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> bool {
        let Ok(text) = std::str::from_utf8(bytes) else { return false };
        let Ok(document) = roxmltree::Document::parse(text) else { return false };
        let root = document.root_element();
        let (vx, vy, vw, vh) = svg_view_box(root).unwrap_or((0.0, 0.0, width as f32, height as f32));
        if vw <= 0.0 || vh <= 0.0 { return false; }
        let scale = (width as f32 / vw).min(height as f32 / vh);
        let dx = (width as f32 - vw * scale) * 0.5;
        let dy = (height as f32 - vh * scale) * 0.5;
        let viewport = SvgAffine {
            a: scale * 2.0 / width as f32,
            b: 0.0,
            c: 0.0,
            d: -scale * 2.0 / height as f32,
            e: -1.0 + (dx - vx * scale) * 2.0 / width as f32,
            f: 1.0 - (dy - vy * scale) * 2.0 / height as f32,
        };
        let mut geometry = VertexBuffers::<GpuSvgVertex, u32>::new();
        let stylesheet = svg_stylesheet(root);
        tessellate_svg_node(
            root,
            SvgPaintState::default(),
            viewport,
            &document,
            &stylesheet,
            &mut geometry,
            0,
        );
        if geometry.indices.is_empty() { return false; }
        use wgpu::util::DeviceExt as _;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fika-svg-vertices"),
            contents: bytemuck::cast_slice(&geometry.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fika-svg-indices"),
            contents: bytemuck::cast_slice(&geometry.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let format = match target.format() {
            wgpu::TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
            _ => return false,
        };
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fika-svg-msaa"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fika-svg-encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fika-svg-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&view),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let pipeline = match format {
                wgpu::TextureFormat::Rgba8Unorm => &self.rgba_pipeline,
                wgpu::TextureFormat::Bgra8Unorm => &self.bgra_pipeline,
                _ => return false,
            };
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..geometry.indices.len() as u32, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        true
    }
}

fn create_gpu_svg_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fika-svg-shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GPU_SVG_SHADER)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fika-svg-pipeline"),
        layout: None,
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[Some(GpuSvgVertex::layout())] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: 4, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn svg_view_box(node: roxmltree::Node<'_, '_>) -> Option<(f32, f32, f32, f32)> {
    if let Some(value) = node.attribute("viewBox") {
        let values = value.split(|c: char| c.is_ascii_whitespace() || c == ',').filter(|v| !v.is_empty()).filter_map(|v| v.parse::<f32>().ok()).collect::<Vec<_>>();
        if values.len() == 4 { return Some((values[0], values[1], values[2], values[3])); }
    }
    let w = svg_number(node.attribute("width")?)?;
    let h = svg_number(node.attribute("height")?)?;
    Some((0.0, 0.0, w, h))
}

fn svg_number(value: &str) -> Option<f32> {
    value.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%').parse().ok()
}

fn tessellate_svg_node(
    node: roxmltree::Node<'_, '_>,
    parent: SvgPaintState,
    viewport: SvgAffine,
    document: &roxmltree::Document<'_>,
    stylesheet: &HashMap<String, Vec<(String, String)>>,
    geometry: &mut VertexBuffers<GpuSvgVertex, u32>,
    depth: usize,
) {
    if !node.is_element() || depth > 32 { return; }
    let mut state = svg_node_state(node, parent, document, stylesheet);
    let tag = node.tag_name().name();
    if matches!(tag, "defs" | "clipPath" | "mask" | "style" | "linearGradient" | "radialGradient" | "stop") { return; }
    if tag == "use" {
        let href = node.attribute("href").or_else(|| {
            node.attributes().find(|attribute| attribute.name() == "href").map(|attribute| attribute.value())
        });
        if let Some(id) = href.and_then(|href| href.strip_prefix('#'))
            && let Some(target) = document.descendants().find(|candidate| candidate.attribute("id") == Some(id))
        {
            let x = node.attribute("x").and_then(svg_number).unwrap_or(0.0);
            let y = node.attribute("y").and_then(svg_number).unwrap_or(0.0);
            state.transform = SvgAffine { e: x, f: y, ..SvgAffine::IDENTITY }.then(state.transform);
            tessellate_svg_node(target, state, viewport, document, stylesheet, geometry, depth + 1);
        }
        return;
    }
    if let Some(path) = svg_node_path(node) {
        let bounds = lyon::algorithms::aabb::bounding_box(path.iter());
        if let Some(paint) = state.fill.clone() {
            let ctor = SvgVertexCtor { transform: state.transform.then(viewport), paint, bounds, opacity: state.opacity };
            let options = FillOptions::default().with_fill_rule(state.fill_rule);
            let _ = FillTessellator::new().tessellate_path(&path, &options, &mut BuffersBuilder::new(geometry, ctor));
        }
        if let Some(paint) = state.stroke.clone() {
            let ctor = SvgVertexCtor { transform: state.transform.then(viewport), paint, bounds, opacity: state.opacity };
            let options = StrokeOptions::default()
                .with_line_width(state.stroke_width.max(0.1))
                .with_line_cap(state.stroke_cap)
                .with_line_join(state.stroke_join);
            let _ = StrokeTessellator::new().tessellate_path(&path, &options, &mut BuffersBuilder::new(geometry, ctor));
        }
    }
    for child in node.children().filter(|child| child.is_element()) {
        tessellate_svg_node(child, state.clone(), viewport, document, stylesheet, geometry, depth + 1);
    }
}

fn svg_node_state(
    node: roxmltree::Node<'_, '_>,
    mut state: SvgPaintState,
    document: &roxmltree::Document<'_>,
    stylesheet: &HashMap<String, Vec<(String, String)>>,
) -> SvgPaintState {
    if let Some(value) = node.attribute("transform") {
        state.transform = parse_svg_transform(value).then(state.transform);
    }
    let mut declarations = Vec::<(String, String)>::new();
    if let Some(classes) = node.attribute("class") {
        for class in classes.split_ascii_whitespace() {
            if let Some(class_declarations) = stylesheet.get(class) {
                declarations.extend(class_declarations.iter().cloned());
            }
        }
    }
    for key in ["color", "fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "fill-rule", "opacity", "fill-opacity", "stroke-opacity"] {
        if let Some(value) = node.attribute(key) { declarations.push((key.to_string(), value.to_string())); }
    }
    if let Some(style) = node.attribute("style") {
        declarations.extend(svg_declaration_list(style));
    }
    for (_, value) in declarations.iter().filter(|(key, _)| key == "color") {
        if let Some(color) = parse_svg_color(value) { state.color = color; }
    }
    for (key, value) in &declarations {
        match key.as_str() {
            "fill" => if value != "inherit" { state.fill = parse_svg_paint(value, state.color, document) },
            "stroke" => if value != "inherit" { state.stroke = parse_svg_paint(value, state.color, document) },
            "stroke-width" => if let Some(v) = svg_number(value) { state.stroke_width = v },
            "stroke-linecap" => state.stroke_cap = match value.as_str() { "round" => LineCap::Round, "square" => LineCap::Square, _ => LineCap::Butt },
            "stroke-linejoin" => state.stroke_join = match value.as_str() { "round" => LineJoin::Round, "bevel" => LineJoin::Bevel, _ => LineJoin::Miter },
            "fill-rule" => state.fill_rule = if value == "evenodd" { FillRule::EvenOdd } else { FillRule::NonZero },
            "opacity" => if let Some(v) = svg_number(value) { state.opacity *= v.clamp(0.0, 1.0) },
            "fill-opacity" => if let Some(v) = svg_number(value) { multiply_svg_paint_alpha(&mut state.fill, v) },
            "stroke-opacity" => if let Some(v) = svg_number(value) { multiply_svg_paint_alpha(&mut state.stroke, v) },
            _ => {}
        }
    }
    state
}

fn svg_stylesheet(root: roxmltree::Node<'_, '_>) -> HashMap<String, Vec<(String, String)>> {
    let mut stylesheet = HashMap::new();
    for style in root.descendants().filter(|node| node.has_tag_name("style")) {
        let Some(text) = style.text() else { continue };
        for rule in text.split('}') {
            let Some((selectors, declarations)) = rule.split_once('{') else { continue };
            let declarations = svg_declaration_list(declarations);
            for selector in selectors.split(',') {
                let selector = selector.trim();
                let Some(class) = selector.strip_prefix('.') else { continue };
                if class.is_empty() || class.chars().any(|character| character.is_ascii_whitespace() || matches!(character, ':' | '[' | '>' | '+')) {
                    continue;
                }
                stylesheet.entry(class.to_string()).or_insert_with(Vec::new).extend(declarations.iter().cloned());
            }
        }
    }
    stylesheet
}

fn svg_declaration_list(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|declaration| declaration.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string()))
        .filter(|(key, value)| !key.is_empty() && !value.is_empty())
        .collect()
}

fn parse_svg_color(value: &str) -> Option<[f32; 4]> {
    let color: svgtypes::Color = value.trim().parse().ok()?;
    Some(svg_color(color))
}

fn svg_color(color: svgtypes::Color) -> [f32; 4] {
    [
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        color.alpha as f32 / 255.0,
    ]
}

fn multiply_svg_paint_alpha(paint: &mut Option<SvgPaint>, opacity: f32) {
    let opacity = opacity.clamp(0.0, 1.0);
    match paint {
        Some(SvgPaint::Solid(color)) => color[3] *= opacity,
        Some(SvgPaint::Linear { stops, .. }) | Some(SvgPaint::Radial { stops, .. }) => {
            let adjusted = stops.iter().cloned().map(|mut stop| { stop.color[3] *= opacity; stop }).collect::<Vec<_>>();
            *stops = adjusted.into();
        }
        None => {}
    }
}

fn parse_svg_gradient(document: &roxmltree::Document<'_>, id: &str) -> Option<SvgPaint> {
    let node = document.descendants().find(|node| node.attribute("id") == Some(id))?;
    let stops = svg_gradient_stops(node, document, 0);
    if stops.is_empty() { return None; }
    let object_bounding_box = svg_gradient_attribute(node, "gradientUnits", document, 0)
        .map_or(true, |units| units != "userSpaceOnUse");
    let transform = svg_gradient_attribute(node, "gradientTransform", document, 0)
        .map(parse_svg_transform)
        .unwrap_or(SvgAffine::IDENTITY);
    match node.tag_name().name() {
        "linearGradient" => Some(SvgPaint::Linear {
            start: [
                svg_gradient_coordinate(svg_gradient_attribute(node, "x1", document, 0), object_bounding_box, 0.0),
                svg_gradient_coordinate(svg_gradient_attribute(node, "y1", document, 0), object_bounding_box, 0.0),
            ],
            end: [
                svg_gradient_coordinate(svg_gradient_attribute(node, "x2", document, 0), object_bounding_box, if object_bounding_box { 1.0 } else { 1.0 }),
                svg_gradient_coordinate(svg_gradient_attribute(node, "y2", document, 0), object_bounding_box, 0.0),
            ],
            object_bounding_box,
            transform,
            stops: stops.into(),
        }),
        "radialGradient" => Some(SvgPaint::Radial {
            center: [
                svg_gradient_coordinate(svg_gradient_attribute(node, "cx", document, 0), object_bounding_box, 0.5),
                svg_gradient_coordinate(svg_gradient_attribute(node, "cy", document, 0), object_bounding_box, 0.5),
            ],
            radius: svg_gradient_coordinate(svg_gradient_attribute(node, "r", document, 0), object_bounding_box, 0.5),
            object_bounding_box,
            transform,
            stops: stops.into(),
        }),
        _ => None,
    }
}

fn svg_gradient_attribute<'a>(
    node: roxmltree::Node<'a, 'a>,
    name: &str,
    document: &'a roxmltree::Document<'a>,
    depth: usize,
) -> Option<&'a str> {
    if let Some(value) = node.attribute(name) { return Some(value); }
    if depth >= 8 { return None; }
    let href = node.attribute("href").or_else(|| node.attributes().find(|attribute| attribute.name() == "href").map(|attribute| attribute.value()))?;
    let id = href.strip_prefix('#')?;
    let parent = document.descendants().find(|candidate| candidate.attribute("id") == Some(id))?;
    svg_gradient_attribute(parent, name, document, depth + 1)
}

fn svg_gradient_stops(
    node: roxmltree::Node<'_, '_>,
    document: &roxmltree::Document<'_>,
    depth: usize,
) -> Vec<SvgGradientStop> {
    let mut stops = node.children().filter(|child| child.has_tag_name("stop")).filter_map(|stop| {
        let mut color = [0.0, 0.0, 0.0, 1.0];
        let mut offset = 0.0;
        let mut declarations = Vec::new();
        for key in ["offset", "stop-color", "stop-opacity"] {
            if let Some(value) = stop.attribute(key) { declarations.push((key.to_string(), value.to_string())); }
        }
        if let Some(style) = stop.attribute("style") { declarations.extend(svg_declaration_list(style)); }
        for (key, value) in declarations {
            match key.as_str() {
                "offset" => offset = svg_percentage_or_number(&value).unwrap_or(0.0).clamp(0.0, 1.0),
                "stop-color" => if let Some(parsed) = parse_svg_color(&value) { color = parsed },
                "stop-opacity" => if let Some(opacity) = svg_number(&value) { color[3] *= opacity.clamp(0.0, 1.0) },
                _ => {}
            }
        }
        Some(SvgGradientStop { offset, color })
    }).collect::<Vec<_>>();
    if stops.is_empty() && depth < 8 {
        let href = node.attribute("href").or_else(|| node.attributes().find(|attribute| attribute.name() == "href").map(|attribute| attribute.value()));
        if let Some(parent) = href.and_then(|href| href.strip_prefix('#')).and_then(|id| document.descendants().find(|candidate| candidate.attribute("id") == Some(id))) {
            stops = svg_gradient_stops(parent, document, depth + 1);
        }
    }
    stops.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    stops
}

fn svg_gradient_coordinate(value: Option<&str>, object_bounding_box: bool, default: f32) -> f32 {
    let Some(value) = value else { return default };
    if value.trim().ends_with('%') {
        return svg_percentage_or_number(value).unwrap_or(default);
    }
    let parsed = svg_number(value).unwrap_or(default);
    if object_bounding_box { parsed } else { parsed }
}

fn svg_percentage_or_number(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        percent.trim().parse::<f32>().ok().map(|number| number / 100.0)
    } else {
        value.parse().ok()
    }
}

fn sample_svg_gradient(stops: &[SvgGradientStop], offset: f32) -> [f32; 4] {
    let Some(first) = stops.first() else { return [0.0; 4] };
    if offset <= first.offset { return first.color; }
    for pair in stops.windows(2) {
        if offset <= pair[1].offset {
            let span = (pair[1].offset - pair[0].offset).max(f32::EPSILON);
            let amount = ((offset - pair[0].offset) / span).clamp(0.0, 1.0);
            return std::array::from_fn(|index| pair[0].color[index] + (pair[1].color[index] - pair[0].color[index]) * amount);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

fn parse_svg_paint(value: &str, current_color: [f32; 4], document: &roxmltree::Document<'_>) -> Option<SvgPaint> {
    match svgtypes::Paint::from_str(value).ok()? {
        svgtypes::Paint::None => None,
        svgtypes::Paint::CurrentColor => Some(SvgPaint::Solid(current_color)),
        svgtypes::Paint::Color(color) => Some(SvgPaint::Solid(svg_color(color))),
        svgtypes::Paint::FuncIRI(id, fallback) => parse_svg_gradient(document, id).or_else(|| match fallback {
            Some(svgtypes::PaintFallback::CurrentColor) => Some(SvgPaint::Solid(current_color)),
            Some(svgtypes::PaintFallback::Color(color)) => Some(SvgPaint::Solid(svg_color(color))),
            _ => None,
        }),
        svgtypes::Paint::Inherit => None,
        svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke => Some(SvgPaint::Solid(current_color)),
    }
}

fn parse_svg_transform(value: &str) -> SvgAffine {
    use svgtypes::TransformListToken as T;
    let mut out = SvgAffine::IDENTITY;
    for token in svgtypes::TransformListParser::from(value).flatten() {
        let next = match token {
            T::Matrix { a, b, c, d, e, f } => SvgAffine { a: a as f32, b: b as f32, c: c as f32, d: d as f32, e: e as f32, f: f as f32 },
            T::Translate { tx, ty } => SvgAffine { e: tx as f32, f: ty as f32, ..SvgAffine::IDENTITY },
            T::Scale { sx, sy } => SvgAffine { a: sx as f32, d: sy as f32, ..SvgAffine::IDENTITY },
            T::Rotate { angle } => { let (s, c) = (angle as f32).to_radians().sin_cos(); SvgAffine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 } },
            T::SkewX { angle } => SvgAffine { c: (angle as f32).to_radians().tan(), ..SvgAffine::IDENTITY },
            T::SkewY { angle } => SvgAffine { b: (angle as f32).to_radians().tan(), ..SvgAffine::IDENTITY },
        };
        out = next.then(out);
    }
    out
}

fn svg_node_path(node: roxmltree::Node<'_, '_>) -> Option<LyonPath> {
    let mut builder = LyonPath::builder().with_svg();
    match node.tag_name().name() {
        "path" => build_svg_path_data(node.attribute("d")?, &mut builder),
        "rect" => {
            let x = node.attribute("x").and_then(svg_number).unwrap_or(0.0); let y = node.attribute("y").and_then(svg_number).unwrap_or(0.0);
            let w = svg_number(node.attribute("width")?)?; let h = svg_number(node.attribute("height")?)?;
            let raw_rx = node.attribute("rx").and_then(svg_number);
            let raw_ry = node.attribute("ry").and_then(svg_number);
            let rx = raw_rx.or(raw_ry).unwrap_or(0.0).clamp(0.0, w * 0.5);
            let ry = raw_ry.or(raw_rx).unwrap_or(0.0).clamp(0.0, h * 0.5);
            if rx > 0.0 || ry > 0.0 {
                builder.move_to(point(x + rx, y));
                builder.line_to(point(x + w - rx, y));
                builder.quadratic_bezier_to(point(x + w, y), point(x + w, y + ry));
                builder.line_to(point(x + w, y + h - ry));
                builder.quadratic_bezier_to(point(x + w, y + h), point(x + w - rx, y + h));
                builder.line_to(point(x + rx, y + h));
                builder.quadratic_bezier_to(point(x, y + h), point(x, y + h - ry));
                builder.line_to(point(x, y + ry));
                builder.quadratic_bezier_to(point(x, y), point(x + rx, y));
            } else {
                builder.move_to(point(x, y)); builder.line_to(point(x + w, y)); builder.line_to(point(x + w, y + h)); builder.line_to(point(x, y + h));
            }
            builder.close();
        }
        "circle" | "ellipse" => {
            let cx = node.attribute("cx").and_then(svg_number).unwrap_or(0.0); let cy = node.attribute("cy").and_then(svg_number).unwrap_or(0.0);
            let rx = if node.tag_name().name() == "circle" { svg_number(node.attribute("r")?)? } else { svg_number(node.attribute("rx")?)? };
            let ry = if node.tag_name().name() == "circle" { rx } else { svg_number(node.attribute("ry")?)? };
            for i in 0..=48 { let a = i as f32 * std::f32::consts::TAU / 48.0; let p = point(cx + rx * a.cos(), cy + ry * a.sin()); if i == 0 { builder.move_to(p); } else { builder.line_to(p); } } builder.close();
        }
        "polygon" | "polyline" => {
            let nums = node.attribute("points")?.split(|c: char| c.is_ascii_whitespace() || c == ',').filter(|v| !v.is_empty()).filter_map(|v| v.parse::<f32>().ok()).collect::<Vec<_>>();
            for (i, pair) in nums.chunks_exact(2).enumerate() { if i == 0 { builder.move_to(point(pair[0], pair[1])); } else { builder.line_to(point(pair[0], pair[1])); } }
            if node.tag_name().name() == "polygon" { builder.close(); }
        }
        "line" => { builder.move_to(point(svg_number(node.attribute("x1")?)?, svg_number(node.attribute("y1")?)?)); builder.line_to(point(svg_number(node.attribute("x2")?)?, svg_number(node.attribute("y2")?)?)); }
        _ => return None,
    }
    Some(builder.build())
}

fn build_svg_path_data(data: &str, builder: &mut impl SvgPathBuilder) {
    use svgtypes::PathSegment as S;
    for segment in svgtypes::PathParser::from(data).flatten() {
        match segment {
            S::MoveTo { abs: true, x, y } => builder.move_to(point(x as f32, y as f32)),
            S::MoveTo { x, y, .. } => builder.relative_move_to(vector(x as f32, y as f32)),
            S::LineTo { abs: true, x, y } => builder.line_to(point(x as f32, y as f32)),
            S::LineTo { x, y, .. } => builder.relative_line_to(vector(x as f32, y as f32)),
            S::HorizontalLineTo { abs: true, x } => builder.horizontal_line_to(x as f32),
            S::HorizontalLineTo { x, .. } => builder.relative_horizontal_line_to(x as f32),
            S::VerticalLineTo { abs: true, y } => builder.vertical_line_to(y as f32),
            S::VerticalLineTo { y, .. } => builder.relative_vertical_line_to(y as f32),
            S::CurveTo { abs: true, x1, y1, x2, y2, x, y } => builder.cubic_bezier_to(point(x1 as f32, y1 as f32), point(x2 as f32, y2 as f32), point(x as f32, y as f32)),
            S::CurveTo { x1, y1, x2, y2, x, y, .. } => builder.relative_cubic_bezier_to(vector(x1 as f32, y1 as f32), vector(x2 as f32, y2 as f32), vector(x as f32, y as f32)),
            S::SmoothCurveTo { abs: true, x2, y2, x, y } => builder.smooth_cubic_bezier_to(point(x2 as f32, y2 as f32), point(x as f32, y as f32)),
            S::SmoothCurveTo { x2, y2, x, y, .. } => builder.smooth_relative_cubic_bezier_to(vector(x2 as f32, y2 as f32), vector(x as f32, y as f32)),
            S::Quadratic { abs: true, x1, y1, x, y } => builder.quadratic_bezier_to(point(x1 as f32, y1 as f32), point(x as f32, y as f32)),
            S::Quadratic { x1, y1, x, y, .. } => builder.relative_quadratic_bezier_to(vector(x1 as f32, y1 as f32), vector(x as f32, y as f32)),
            S::SmoothQuadratic { abs: true, x, y } => builder.smooth_quadratic_bezier_to(point(x as f32, y as f32)),
            S::SmoothQuadratic { x, y, .. } => builder.smooth_relative_quadratic_bezier_to(vector(x as f32, y as f32)),
            S::EllipticalArc { abs: true, rx, ry, x_axis_rotation, large_arc, sweep, x, y } => builder.arc_to(vector(rx as f32, ry as f32), Angle::degrees(x_axis_rotation as f32), lyon::path::ArcFlags { large_arc, sweep }, point(x as f32, y as f32)),
            S::EllipticalArc { rx, ry, x_axis_rotation, large_arc, sweep, x, y, .. } => builder.relative_arc_to(vector(rx as f32, ry as f32), Angle::degrees(x_axis_rotation as f32), lyon::path::ArcFlags { large_arc, sweep }, vector(x as f32, y as f32)),
            S::ClosePath { .. } => builder.close(),
        }
    }
}
