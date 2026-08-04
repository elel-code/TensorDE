enum LoadedIconSource {
    Svg {
        bytes: Vec<u8>,
        intrinsic: SvgIntrinsicSize,
    },
    Bitmap {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
}

impl LoadedIconSource {
    fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        if is_svg_path(path) {
            let intrinsic = svg_intrinsic_size(&bytes)?;
            Some(Self::Svg { bytes, intrinsic })
        } else {
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (width, height) = image.dimensions();
            if width == 0 || height == 0 {
                return None;
            }
            let mut pixels = image.into_raw();
            premultiply_rgba8(&mut pixels);
            Some(Self::Bitmap {
                width,
                height,
                pixels,
            })
        }
    }

    fn intrinsic_size(&self) -> SvgIntrinsicSize {
        match self {
            Self::Svg { intrinsic, .. } => *intrinsic,
            Self::Bitmap { width, height, .. } => SvgIntrinsicSize {
                width: *width as f32,
                height: *height as f32,
            },
        }
    }
}

fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn premultiply_rgba8(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        pixel[0] = ((u16::from(pixel[0]) * alpha + 127) / 255) as u8;
        pixel[1] = ((u16::from(pixel[1]) * alpha + 127) / 255) as u8;
        pixel[2] = ((u16::from(pixel[2]) * alpha + 127) / 255) as u8;
    }
}
