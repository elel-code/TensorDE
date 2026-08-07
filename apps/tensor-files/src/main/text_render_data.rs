#[cfg(test)]
fn text_vertices_for_pending(
    pending: &[PendingTextDraw],
    atlases: &[AtlasRect],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(pending.len() * 6);
    for (draw, atlas) in pending.iter().zip(atlases.iter()) {
        push_pending_text_vertices(
            &mut vertices,
            draw,
            *atlas,
            atlas_width,
            atlas_height,
            surface_size,
        );
    }
    vertices
}

#[cfg(test)]
fn text_vertices_for_pending_indices(
    pending: &[PendingTextDraw],
    drawable_indices: &[usize],
    atlases: &[AtlasRect],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(drawable_indices.len() * 6);
    text_vertices_for_pending_indices_into(
        &mut vertices,
        pending,
        drawable_indices,
        atlases,
        atlas_width,
        atlas_height,
        surface_size,
    );
    vertices
}

fn text_vertices_for_pending_indices_into(
    vertices: &mut Vec<TextVertex>,
    pending: &[PendingTextDraw],
    drawable_indices: &[usize],
    atlases: &[AtlasRect],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) {
    debug_assert_eq!(drawable_indices.len(), atlases.len());
    vertices.reserve(drawable_indices.len().saturating_mul(6));
    for (&draw_index, atlas) in drawable_indices.iter().zip(atlases.iter()) {
        push_pending_text_vertices(
            vertices,
            &pending[draw_index],
            *atlas,
            atlas_width,
            atlas_height,
            surface_size,
        );
    }
}

fn push_pending_text_vertices(
    vertices: &mut Vec<TextVertex>,
    draw: &PendingTextDraw,
    atlas: AtlasRect,
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) {
    let guard = TEXT_ATLAS_GUARD_TEXELS as f32;
    let scale_x = draw.label_width as f32 / draw.rect.width.max(1.0);
    let scale_y = draw.label_height as f32 / draw.rect.height.max(1.0);
    let atlas = AtlasRect {
        x: atlas.x + guard + (draw.screen.x - draw.rect.x).max(0.0) * scale_x,
        y: atlas.y + guard + (draw.screen.y - draw.rect.y).max(0.0) * scale_y,
        width: draw.screen.width * scale_x,
        height: draw.screen.height * scale_y,
    };
    push_textured_rect(
        vertices,
        draw.screen,
        atlas,
        atlas_width,
        atlas_height,
        surface_size,
        text_color_to_vertex_color(draw.color),
    );
}

fn text_atlas_upload_should_skip(
    upload: &TextAtlasUpload,
    last_upload_keys: &HashSet<TextAtlasUploadKey>,
    current_upload_keys: &mut HashSet<TextAtlasUploadKey>,
) -> bool {
    let key = TextAtlasUploadKey::from_upload(upload);
    let skip_upload = last_upload_keys.contains(&key);
    current_upload_keys.insert(key);
    skip_upload
}

fn text_color_to_vertex_color(color: TextColor) -> [f32; 4] {
    let [r, g, b, a] = color.as_rgba();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}

#[derive(Clone, Copy, Debug)]
struct TextAlphaRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn fill_text_alpha_pixels(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: TextAlphaRect,
    color: TextColor,
) {
    if color.a() == 0 || rect.width == 0 || rect.height == 0 {
        return;
    }
    let x0 = rect.x.max(0) as u32;
    let y0 = rect.y.max(0) as u32;
    let x1 = rect
        .x
        .saturating_add(rect.width as i32)
        .clamp(0, width as i32) as u32;
    let y1 = rect
        .y
        .saturating_add(rect.height as i32)
        .clamp(0, height as i32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let offset = (y * width + x) as usize;
            pixels[offset] = blend_alpha(pixels[offset], color.a());
        }
    }
}

fn blend_alpha(destination: u8, source: u8) -> u8 {
    let source = f32::from(source) / 255.0;
    let destination = f32::from(destination) / 255.0;
    ((source + destination * (1.0 - source)) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn shaping_for_label(label: &str, wrap: LabelWrap) -> Shaping {
    if wrap == LabelWrap::None && label.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    }
}

#[derive(Clone, Debug)]
struct IconThemeResolver {
    roots: Vec<PathBuf>,
    themes: Vec<String>,
    search_order: Option<Vec<String>>,
    inherits_cache: HashMap<String, Vec<String>>,
    path_cache: HashMap<String, HashMap<u16, Option<Arc<Path>>>>,
    dir_exists_cache: HashMap<PathBuf, bool>,
    renderable_file_cache: HashMap<PathBuf, bool>,
}

impl Default for IconThemeResolver {
    fn default() -> Self {
        Self {
            roots: icon_theme_roots(),
            themes: icon_theme_names(),
            search_order: None,
            inherits_cache: HashMap::new(),
            path_cache: HashMap::new(),
            dir_exists_cache: HashMap::new(),
            renderable_file_cache: HashMap::new(),
        }
    }
}
