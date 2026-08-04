use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const MAX_SECTIONS: u16 = 96;
const MAX_RESOURCE_SECTION_BYTES: u32 = 64 * 1024 * 1024;
const PE32_MAGIC: u16 = 0x10b;
const PE32_PLUS_MAGIC: u16 = 0x20b;
const PE32_DATA_DIRECTORY_OFFSET: usize = 96;
const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 112;
const RESOURCE_DIRECTORY_INDEX: usize = 2;
const RESOURCE_DIRECTORY_ENTRY_SIZE: usize = 8;
const RESOURCE_DIRECTORY_HEADER_SIZE: usize = 16;
const RESOURCE_DATA_ENTRY_SIZE: usize = 16;
const RESOURCE_DIRECTORY_FLAG: u32 = 0x8000_0000;
const RESOURCE_OFFSET_MASK: u32 = 0x7fff_ffff;
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

pub fn windows_executable_icon_ico(
    path: &Path,
    preferred_dimension: u16,
) -> io::Result<Option<Vec<u8>>> {
    let Some(section) = read_resource_section(path)? else {
        return Ok(None);
    };
    Ok(icon_ico_from_resource_section(
        &section,
        preferred_dimension,
    ))
}

#[derive(Clone, Debug)]
struct ResourceSection {
    rva: u32,
    data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct SectionHeader {
    virtual_address: u32,
    virtual_size: u32,
    raw_data_offset: u32,
    raw_data_size: u32,
}

#[derive(Clone, Copy, Debug)]
struct ResourceDirectoryEntry {
    name_or_id: u32,
    offset: u32,
}

#[derive(Clone, Debug)]
struct GroupIconEntry {
    width: u8,
    height: u8,
    color_count: u8,
    reserved: u8,
    planes: u16,
    bit_count: u16,
    bytes_in_resource: u32,
    icon_id: u16,
}

fn read_resource_section(path: &Path) -> io::Result<Option<ResourceSection>> {
    let mut file = File::open(path)?;
    let mut dos = [0u8; 64];
    if file.read_exact(&mut dos).is_err() {
        return Ok(None);
    }
    if &dos[0..2] != b"MZ" {
        return Ok(None);
    }

    let pe_offset = read_u32(&dos, 0x3c).unwrap_or_default() as u64;
    file.seek(SeekFrom::Start(pe_offset))?;
    let mut pe_header = [0u8; 24];
    if file.read_exact(&mut pe_header).is_err() {
        return Ok(None);
    }
    if &pe_header[0..4] != b"PE\0\0" {
        return Ok(None);
    }

    let section_count = read_u16(&pe_header, 6).unwrap_or_default();
    let optional_header_size = read_u16(&pe_header, 20).unwrap_or_default() as usize;
    if section_count == 0 || section_count > MAX_SECTIONS || optional_header_size == 0 {
        return Ok(None);
    }

    let mut optional_header = vec![0u8; optional_header_size];
    if file.read_exact(&mut optional_header).is_err() {
        return Ok(None);
    }
    let Some((resource_rva, resource_size)) =
        resource_directory_from_optional_header(&optional_header)
    else {
        return Ok(None);
    };
    if resource_rva == 0 || resource_size == 0 {
        return Ok(None);
    }

    let mut section_headers = vec![0u8; section_count as usize * 40];
    if file.read_exact(&mut section_headers).is_err() {
        return Ok(None);
    }
    let sections = parse_section_headers(&section_headers);
    let Some(section) = section_for_rva(&sections, resource_rva) else {
        return Ok(None);
    };
    let delta = resource_rva.saturating_sub(section.virtual_address);
    if delta >= section.raw_data_size {
        return Ok(None);
    }
    let raw_offset = section.raw_data_offset.saturating_add(delta);
    let read_size = resource_size
        .min(section.raw_data_size.saturating_sub(delta))
        .min(MAX_RESOURCE_SECTION_BYTES);
    if read_size < RESOURCE_DIRECTORY_HEADER_SIZE as u32 {
        return Ok(None);
    }

    let mut data = vec![0u8; read_size as usize];
    file.seek(SeekFrom::Start(raw_offset as u64))?;
    if file.read_exact(&mut data).is_err() {
        return Ok(None);
    }
    Ok(Some(ResourceSection {
        rva: resource_rva,
        data,
    }))
}

fn resource_directory_from_optional_header(header: &[u8]) -> Option<(u32, u32)> {
    let magic = read_u16(header, 0)?;
    let directory_offset = match magic {
        PE32_MAGIC => PE32_DATA_DIRECTORY_OFFSET,
        PE32_PLUS_MAGIC => PE32_PLUS_DATA_DIRECTORY_OFFSET,
        _ => return None,
    };
    let resource_entry = directory_offset + RESOURCE_DIRECTORY_INDEX * 8;
    Some((
        read_u32(header, resource_entry)?,
        read_u32(header, resource_entry + 4)?,
    ))
}

fn parse_section_headers(bytes: &[u8]) -> Vec<SectionHeader> {
    bytes
        .chunks_exact(40)
        .filter_map(|chunk| {
            Some(SectionHeader {
                virtual_size: read_u32(chunk, 8)?,
                virtual_address: read_u32(chunk, 12)?,
                raw_data_size: read_u32(chunk, 16)?,
                raw_data_offset: read_u32(chunk, 20)?,
            })
        })
        .collect()
}

fn section_for_rva(sections: &[SectionHeader], rva: u32) -> Option<SectionHeader> {
    sections.iter().copied().find(|section| {
        let size = section.virtual_size.max(section.raw_data_size);
        rva >= section.virtual_address && rva < section.virtual_address.saturating_add(size.max(1))
    })
}

fn icon_ico_from_resource_section(
    section: &ResourceSection,
    max_dimension: u16,
) -> Option<Vec<u8>> {
    let mut groups = resource_payloads_for_type(section, RT_GROUP_ICON);
    groups.sort_by_key(|payload| std::cmp::Reverse(payload.len()));
    for group in groups {
        let mut entries = parse_group_icon_entries(group);
        entries.sort_by_key(|entry| icon_choice_score(entry, max_dimension));
        for entry in entries {
            let Some(icon_data) =
                first_resource_payload_for_type_named_id(section, RT_ICON, entry.icon_id)
            else {
                continue;
            };
            if icon_data.len() != entry.bytes_in_resource as usize {
                continue;
            }
            return Some(single_image_ico(&entry, icon_data));
        }
    }
    None
}

fn resource_payloads_for_type(section: &ResourceSection, type_id: u16) -> Vec<&[u8]> {
    let Some(entry) = resource_directory_entries(&section.data, 0)
        .into_iter()
        .find(|entry| entry.id() == Some(type_id))
    else {
        return Vec::new();
    };
    let mut payloads = Vec::new();
    collect_resource_payloads(section, entry, 0, &mut payloads);
    payloads
}

fn first_resource_payload_for_type_named_id(
    section: &ResourceSection,
    type_id: u16,
    name_id: u16,
) -> Option<&[u8]> {
    let type_entry = resource_directory_entries(&section.data, 0)
        .into_iter()
        .find(|entry| entry.id() == Some(type_id))?;
    if !type_entry.is_directory() {
        return None;
    }
    let named_entry = resource_directory_entries(&section.data, type_entry.target_offset())
        .into_iter()
        .find(|entry| entry.id() == Some(name_id))?;
    let mut payloads = Vec::new();
    collect_resource_payloads(section, named_entry, 0, &mut payloads);
    payloads.into_iter().next()
}

fn collect_resource_payloads<'a>(
    section: &'a ResourceSection,
    entry: ResourceDirectoryEntry,
    depth: usize,
    payloads: &mut Vec<&'a [u8]>,
) {
    if depth > 4 {
        return;
    }
    if entry.is_directory() {
        for child in resource_directory_entries(&section.data, entry.target_offset()) {
            collect_resource_payloads(section, child, depth + 1, payloads);
        }
        return;
    }
    if let Some(payload) = resource_data_entry_payload(section, entry.target_offset()) {
        payloads.push(payload);
    }
}

fn resource_directory_entries(data: &[u8], offset: u32) -> Vec<ResourceDirectoryEntry> {
    let offset = offset as usize;
    let Some(header) = data.get(offset..offset + RESOURCE_DIRECTORY_HEADER_SIZE) else {
        return Vec::new();
    };
    let named = read_u16(header, 12).unwrap_or_default() as usize;
    let ids = read_u16(header, 14).unwrap_or_default() as usize;
    let count = named.saturating_add(ids);
    let entries_start = offset + RESOURCE_DIRECTORY_HEADER_SIZE;
    let Some(entries_end) = count
        .checked_mul(RESOURCE_DIRECTORY_ENTRY_SIZE)
        .and_then(|len| entries_start.checked_add(len))
    else {
        return Vec::new();
    };
    let Some(entries) = data.get(entries_start..entries_end) else {
        return Vec::new();
    };
    entries
        .chunks_exact(RESOURCE_DIRECTORY_ENTRY_SIZE)
        .filter_map(|chunk| {
            Some(ResourceDirectoryEntry {
                name_or_id: read_u32(chunk, 0)?,
                offset: read_u32(chunk, 4)?,
            })
        })
        .collect()
}

fn resource_data_entry_payload(section: &ResourceSection, data_entry_offset: u32) -> Option<&[u8]> {
    let offset = data_entry_offset as usize;
    let entry = section
        .data
        .get(offset..offset + RESOURCE_DATA_ENTRY_SIZE)?;
    let data_rva = read_u32(entry, 0)?;
    let size = read_u32(entry, 4)? as usize;
    let data_offset = data_rva.checked_sub(section.rva)? as usize;
    section
        .data
        .get(data_offset..data_offset.checked_add(size)?)
}

impl ResourceDirectoryEntry {
    fn id(self) -> Option<u16> {
        (self.name_or_id & RESOURCE_DIRECTORY_FLAG == 0).then_some(self.name_or_id as u16)
    }

    fn is_directory(self) -> bool {
        self.offset & RESOURCE_DIRECTORY_FLAG != 0
    }

    fn target_offset(self) -> u32 {
        self.offset & RESOURCE_OFFSET_MASK
    }
}

fn parse_group_icon_entries(bytes: &[u8]) -> Vec<GroupIconEntry> {
    if bytes.len() < 6 || read_u16(bytes, 0) != Some(0) || read_u16(bytes, 2) != Some(1) {
        return Vec::new();
    }
    let count = read_u16(bytes, 4).unwrap_or_default() as usize;
    let mut entries = Vec::new();
    for index in 0..count {
        let offset = 6 + index * 14;
        let Some(entry) = bytes.get(offset..offset + 14) else {
            break;
        };
        entries.push(GroupIconEntry {
            width: entry[0],
            height: entry[1],
            color_count: entry[2],
            reserved: entry[3],
            planes: read_u16(entry, 4).unwrap_or_default(),
            bit_count: read_u16(entry, 6).unwrap_or_default(),
            bytes_in_resource: read_u32(entry, 8).unwrap_or_default(),
            icon_id: read_u16(entry, 12).unwrap_or_default(),
        });
    }
    entries
}

fn icon_choice_score(entry: &GroupIconEntry, max_dimension: u16) -> (u32, std::cmp::Reverse<u16>) {
    let target = u32::from(max_dimension.clamp(16, 256));
    let dimension = entry.display_width().max(entry.display_height());
    let distance = if dimension >= target {
        dimension - target
    } else {
        target - dimension + 1024
    };
    (distance, std::cmp::Reverse(entry.bit_count))
}

fn single_image_ico(entry: &GroupIconEntry, icon_data: &[u8]) -> Vec<u8> {
    let mut ico = Vec::with_capacity(6 + 16 + icon_data.len());
    ico.extend([0, 0, 1, 0, 1, 0]);
    ico.push(entry.width);
    ico.push(entry.height);
    ico.push(entry.color_count);
    ico.push(entry.reserved);
    ico.extend(entry.planes.to_le_bytes());
    ico.extend(entry.bit_count.to_le_bytes());
    ico.extend((icon_data.len() as u32).to_le_bytes());
    ico.extend(22u32.to_le_bytes());
    ico.extend(icon_data);
    ico
}

impl GroupIconEntry {
    fn display_width(&self) -> u32 {
        icon_entry_dimension(self.width)
    }

    fn display_height(&self) -> u32 {
        icon_entry_dimension(self.height)
    }
}

fn icon_entry_dimension(value: u8) -> u32 {
    if value == 0 { 256 } else { u32::from(value) }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    const RESOURCE_RVA: u32 = 0x1000;
    const RESOURCE_RAW_OFFSET: usize = 0x200;

    #[test]
    fn extracts_png_icon_from_windows_executable_resources() {
        let root = temp_root("pe-icon-extract");
        fs::create_dir_all(&root).unwrap();
        let exe = root.join("app.exe");
        fs::write(&exe, test_pe_with_png_icon([32, 96, 180, 255])).unwrap();

        let ico = windows_executable_icon_ico(&exe, 64).unwrap().unwrap();
        let image = image::load_from_memory_with_format(&ico, image::ImageFormat::Ico)
            .unwrap()
            .into_rgba8();

        assert_eq!(image.width(), 32);
        assert_eq!(image.height(), 32);
        assert_eq!(image.get_pixel(8, 8).0, [32, 96, 180, 255]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pe_files_without_resource_icons_do_not_produce_thumbnail_png() {
        let root = temp_root("pe-icon-missing");
        fs::create_dir_all(&root).unwrap();
        let exe = root.join("empty.exe");
        fs::write(&exe, test_pe_without_resources()).unwrap();

        assert!(windows_executable_icon_ico(&exe, 64).unwrap().is_none());

        let _ = fs::remove_dir_all(root);
    }

    fn test_pe_with_png_icon(color: [u8; 4]) -> Vec<u8> {
        let icon = test_png_icon(32, 32, color);
        let group = test_group_icon_resource(&icon);
        let resource = test_resource_section(&icon, &group);
        test_pe_with_resource_section(&resource)
    }

    fn test_pe_without_resources() -> Vec<u8> {
        let mut pe = test_pe_headers(0);
        pe.resize(RESOURCE_RAW_OFFSET, 0);
        pe
    }

    fn test_pe_with_resource_section(resource: &[u8]) -> Vec<u8> {
        let mut pe = test_pe_headers(resource.len() as u32);
        pe.resize(RESOURCE_RAW_OFFSET, 0);
        pe.extend(resource);
        pe
    }

    fn test_pe_headers(resource_size: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x80];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes.extend(b"PE\0\0");
        bytes.extend(0x14cu16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend([0u8; 12]);
        bytes.extend(0xe0u16.to_le_bytes());
        bytes.extend(0x010fu16.to_le_bytes());

        let optional_start = bytes.len();
        bytes.resize(optional_start + 0xe0, 0);
        bytes[optional_start..optional_start + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        bytes[optional_start + 92..optional_start + 96].copy_from_slice(&16u32.to_le_bytes());
        let resource_entry =
            optional_start + PE32_DATA_DIRECTORY_OFFSET + RESOURCE_DIRECTORY_INDEX * 8;
        if resource_size > 0 {
            bytes[resource_entry..resource_entry + 4].copy_from_slice(&RESOURCE_RVA.to_le_bytes());
            bytes[resource_entry + 4..resource_entry + 8]
                .copy_from_slice(&resource_size.to_le_bytes());
        }

        let mut section = [0u8; 40];
        section[0..5].copy_from_slice(b".rsrc");
        section[8..12].copy_from_slice(&resource_size.to_le_bytes());
        section[12..16].copy_from_slice(&RESOURCE_RVA.to_le_bytes());
        section[16..20].copy_from_slice(&resource_size.to_le_bytes());
        section[20..24].copy_from_slice(&(RESOURCE_RAW_OFFSET as u32).to_le_bytes());
        bytes.extend(section);
        bytes
    }

    fn test_resource_section(icon: &[u8], group: &[u8]) -> Vec<u8> {
        let icon_data_offset = 160usize;
        let group_data_offset = icon_data_offset + icon.len();
        let mut data = vec![0u8; icon_data_offset];

        write_resource_dir(
            &mut data,
            0,
            &[(RT_ICON, 32, true), (RT_GROUP_ICON, 96, true)],
        );
        write_resource_dir(&mut data, 32, &[(1, 56, true)]);
        write_resource_dir(&mut data, 56, &[(1033, 80, false)]);
        write_resource_data_entry(
            &mut data,
            80,
            RESOURCE_RVA + icon_data_offset as u32,
            icon.len(),
        );
        write_resource_dir(&mut data, 96, &[(1, 120, true)]);
        write_resource_dir(&mut data, 120, &[(1033, 144, false)]);
        write_resource_data_entry(
            &mut data,
            144,
            RESOURCE_RVA + group_data_offset as u32,
            group.len(),
        );

        data.extend(icon);
        data.extend(group);
        data
    }

    fn write_resource_dir(data: &mut [u8], offset: usize, entries: &[(u16, u32, bool)]) {
        data[offset + 14..offset + 16].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        let mut entry_offset = offset + RESOURCE_DIRECTORY_HEADER_SIZE;
        for (id, target, directory) in entries {
            data[entry_offset..entry_offset + 4].copy_from_slice(&u32::from(*id).to_le_bytes());
            let target = if *directory {
                RESOURCE_DIRECTORY_FLAG | *target
            } else {
                *target
            };
            data[entry_offset + 4..entry_offset + 8].copy_from_slice(&target.to_le_bytes());
            entry_offset += RESOURCE_DIRECTORY_ENTRY_SIZE;
        }
    }

    fn write_resource_data_entry(data: &mut [u8], offset: usize, rva: u32, size: usize) {
        data[offset..offset + 4].copy_from_slice(&rva.to_le_bytes());
        data[offset + 4..offset + 8].copy_from_slice(&(size as u32).to_le_bytes());
    }

    fn test_group_icon_resource(icon: &[u8]) -> Vec<u8> {
        let mut group = Vec::new();
        group.extend([0, 0, 1, 0, 1, 0]);
        group.extend([32, 32, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((icon.len() as u32).to_le_bytes());
        group.extend(1u16.to_le_bytes());
        group
    }

    fn test_png_icon(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba(color));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "tensor-files-{name}-{}-{nonce}",
            std::process::id()
        ))
    }
}
