fn split_exec_template(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn expand_thumbnailer_exec_token(
    token: &str,
    input: &Path,
    uri: &str,
    output: &Path,
    size: ThumbnailSize,
) -> OsString {
    let mut expanded = String::new();
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            expanded.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => expanded.push('%'),
            Some('i' | 'f' | 'F') => expanded.push_str(&input.to_string_lossy()),
            Some('u' | 'U') => expanded.push_str(uri),
            Some('o') => expanded.push_str(&output.to_string_lossy()),
            Some('s') => expanded.push_str(&size.max_dimension().to_string()),
            Some('d' | 'D') => {
                if let Some(parent) = input.parent() {
                    expanded.push_str(&parent.to_string_lossy());
                }
            }
            Some('n' | 'N') => {
                if let Some(name) = input.file_name() {
                    expanded.push_str(&name.to_string_lossy());
                }
            }
            Some(other) => {
                expanded.push('%');
                expanded.push(other);
            }
            None => expanded.push('%'),
        }
    }
    OsString::from(expanded)
}

fn program_exists_in_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

pub fn write_thumbnail_metadata(path: &Path, uri: &str, modified_secs: u64) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let bytes = thumbnail_png_with_appended_metadata(&bytes, uri, modified_secs)?;
    fs::write(path, bytes)
}

fn failure_thumbnail_png(uri: &str, modified_secs: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(PNG_SIGNATURE);
    bytes.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
    bytes.extend(png_text_chunk("Thumb::URI", uri));
    bytes.extend(png_text_chunk("Thumb::MTime", &modified_secs.to_string()));
    bytes.extend(png_chunk(b"IDAT", FAILURE_THUMBNAIL_IDAT));
    bytes.extend(png_chunk(b"IEND", &[]));
    bytes
}

fn thumbnail_png_with_appended_metadata(
    bytes: &[u8],
    uri: &str,
    modified_secs: u64,
) -> io::Result<Vec<u8>> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail is not a PNG file",
        ));
    }

    let mut offset = PNG_SIGNATURE.len();
    while bytes.len().saturating_sub(offset) >= PNG_CHUNK_HEADER_LEN {
        let chunk_start = offset;
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += PNG_CHUNK_HEADER_LEN;
        let Some(data_end) = offset.checked_add(length) else {
            return Err(invalid_png_thumbnail());
        };
        let Some(next_offset) = data_end.checked_add(PNG_CHUNK_CRC_LEN) else {
            return Err(invalid_png_thumbnail());
        };
        if next_offset > bytes.len() {
            return Err(invalid_png_thumbnail());
        }
        if chunk_type == b"IEND" {
            let mut output = Vec::with_capacity(bytes.len() + uri.len() + 80);
            output.extend(&bytes[..chunk_start]);
            output.extend(png_text_chunk("Thumb::URI", uri));
            output.extend(png_text_chunk("Thumb::MTime", &modified_secs.to_string()));
            output.extend(&bytes[chunk_start..]);
            return Ok(output);
        }
        offset = next_offset;
    }

    Err(invalid_png_thumbnail())
}

fn temporary_thumbnail_path(output_path: &Path) -> PathBuf {
    let mut file_name = output_path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("thumbnail.png"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    file_name.push(format!(".fika-{}-{nonce}.tmp", process::id()));
    output_path.with_file_name(file_name)
}

fn image_thumbnail_extension(extension: &str) -> bool {
    matches!(
        extension,
        "png"
            | "apng"
            | "jpg"
            | "jpeg"
            | "jxl"
            | "gif"
            | "bmp"
            | "tif"
            | "tiff"
            | "webp"
            | "svg"
            | "svgz"
            | "heic"
            | "heif"
            | "avif"
            | "avifs"
    )
}

fn video_thumbnail_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "flv" | "ogv" | "mpeg" | "mpg" | "wmv"
    )
}

fn document_thumbnail_extension(extension: &str) -> bool {
    matches!(extension, "pdf" | "ps" | "eps" | "epub")
}

fn png_text_chunk(key: &str, value: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(key.as_bytes());
    data.push(0);
    data.extend(value.as_bytes());
    png_chunk(b"tEXt", &data)
}

fn png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend((data.len() as u32).to_be_bytes());
    chunk.extend(chunk_type);
    chunk.extend(data);
    chunk.extend(png_crc32(chunk_type, data).to_be_bytes());
    chunk
}

fn png_crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in chunk_type.iter().chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn default_cache_home() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"))
}

fn thumbnail_metadata_from_bytes(bytes: &[u8]) -> io::Result<ThumbnailMetadata> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail is not a PNG file",
        ));
    }

    let mut metadata = ThumbnailMetadata::default();
    let mut offset = PNG_SIGNATURE.len();
    while bytes.len().saturating_sub(offset) >= PNG_CHUNK_HEADER_LEN {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += PNG_CHUNK_HEADER_LEN;
        let Some(data_end) = offset.checked_add(length) else {
            return Err(invalid_png_thumbnail());
        };
        let Some(next_offset) = data_end.checked_add(PNG_CHUNK_CRC_LEN) else {
            return Err(invalid_png_thumbnail());
        };
        if next_offset > bytes.len() {
            return Err(invalid_png_thumbnail());
        }

        let data = &bytes[offset..data_end];
        if chunk_type == b"tEXt" {
            read_thumbnail_text_chunk(data, &mut metadata);
        }

        offset = next_offset;
        if chunk_type == b"IEND" {
            break;
        }
    }
    Ok(metadata)
}

fn read_thumbnail_text_chunk(data: &[u8], metadata: &mut ThumbnailMetadata) {
    let Some(separator) = data.iter().position(|byte| *byte == 0) else {
        return;
    };
    let key = &data[..separator];
    let value = &data[separator + 1..];
    match key {
        b"Thumb::URI" => {
            if let Ok(uri) = std::str::from_utf8(value) {
                metadata.uri = Some(uri.to_string());
            }
        }
        b"Thumb::MTime" => {
            if let Ok(value) = std::str::from_utf8(value)
                && let Ok(mtime) = value.parse::<u64>()
            {
                metadata.mtime = Some(mtime);
            }
        }
        _ => {}
    }
}

fn invalid_png_thumbnail() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "thumbnail PNG has truncated chunk data",
    )
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut message = input.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend(bit_len.to_le_bytes());

    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_le_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }

        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(words[g])
                    .rotate_left(S[i]),
            );
            a = next;
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    bytes_to_hex(&digest)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

