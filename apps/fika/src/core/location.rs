use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::network::{
    NETWORK_ROOT_URI, network_child_path, network_parent_path, network_path_from_uri,
    network_root_path, network_uri_from_path, parse_network_location,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreadcrumbSegment {
    pub label: String,
    pub path: PathBuf,
}

pub fn resolve_location_input(current_dir: &Path, input: &str) -> Option<PathBuf> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if let Ok(path) = network_path_from_uri(input) {
        return Some(path);
    }
    if network_uri_from_path(current_dir).is_some() {
        return resolve_network_relative_input(current_dir, input);
    }
    let expanded = expand_user_path(input);
    if expanded.is_absolute() {
        Some(expanded)
    } else {
        Some(current_dir.join(expanded))
    }
}

pub fn complete_location_input(current_dir: &Path, input: &str) -> Option<String> {
    if network_uri_from_path(current_dir).is_some() || network_path_from_uri(input).is_ok() {
        return None;
    }
    let (parent_text, prefix) = split_location_input(input);
    let parent = if parent_text.is_empty() {
        current_dir.to_path_buf()
    } else {
        resolve_location_input(current_dir, parent_text)?
    };
    let mut matches = fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(prefix)
                .then(|| (name, entry.file_type().ok().is_some_and(|ty| ty.is_dir())))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0));

    let (name, is_dir) = matches.into_iter().next()?;
    let mut completed = join_location_text(parent_text, &name);
    if is_dir && !completed.ends_with('/') {
        completed.push('/');
    }
    Some(completed)
}

pub fn breadcrumb_segments(path: &Path) -> Vec<BreadcrumbSegment> {
    if let Some(segments) = network_breadcrumb_segments(path) {
        return segments;
    }

    let mut segments = Vec::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        let label = match component {
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
                prefix.as_os_str().to_string_lossy().into_owned()
            }
            Component::RootDir => {
                current = PathBuf::from("/");
                "/".to_string()
            }
            Component::CurDir => {
                current.push(".");
                ".".to_string()
            }
            Component::ParentDir => {
                current.push("..");
                "..".to_string()
            }
            Component::Normal(name) => {
                current.push(name);
                name.to_string_lossy().into_owned()
            }
        };
        segments.push(BreadcrumbSegment {
            label,
            path: current.clone(),
        });
    }

    if segments.is_empty() {
        segments.push(BreadcrumbSegment {
            label: ".".to_string(),
            path: PathBuf::from("."),
        });
    }

    segments
}

pub fn normalize_start_dir(path: PathBuf) -> PathBuf {
    if network_uri_from_path(&path).is_some() {
        return path;
    }
    if path.is_dir() {
        path
    } else {
        path.parent()
            .map(|parent| {
                if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                }
            })
            .unwrap_or_else(home_dir)
    }
}

pub fn parent_location(path: &Path) -> Option<PathBuf> {
    network_parent_path(path).or_else(|| path.parent().map(Path::to_path_buf))
}

fn resolve_network_relative_input(current_dir: &Path, input: &str) -> Option<PathBuf> {
    let input = input.trim_matches('/');
    if input.is_empty() || input == "." {
        return Some(current_dir.to_path_buf());
    }
    if input == ".." {
        return network_parent_path(current_dir);
    }

    let mut current = current_dir.to_path_buf();
    for segment in input.split('/').filter(|segment| !segment.is_empty()) {
        if segment == "." {
            continue;
        }
        if segment == ".." {
            current = network_parent_path(&current)?;
        } else {
            current = network_child_path(&current, segment)?;
        }
    }
    Some(current)
}

fn network_breadcrumb_segments(path: &Path) -> Option<Vec<BreadcrumbSegment>> {
    let uri = network_uri_from_path(path)?;
    let mut segments = vec![BreadcrumbSegment {
        label: "Network".to_string(),
        path: network_root_path(),
    }];
    if uri == NETWORK_ROOT_URI {
        return Some(segments);
    }

    let (_, rest) = uri.split_once(':')?;
    let after_slashes = rest.strip_prefix("//")?;
    let (authority, path_and_tail) = after_slashes
        .split_once('/')
        .map_or((after_slashes, ""), |(authority, path)| (authority, path));
    let path_without_tail = path_and_tail
        .split(['?', '#'])
        .next()
        .unwrap_or(path_and_tail)
        .trim_matches('/');
    let mut current = format!("{}://{authority}/", uri.split_once(':')?.0);
    if path_without_tail.is_empty() {
        segments.push(BreadcrumbSegment {
            label: parse_network_location(&current)
                .map(|location| location.display_name)
                .unwrap_or_else(|_| authority.to_string()),
            path: PathBuf::from(current),
        });
        return Some(segments);
    }

    let mut first = true;
    for segment in path_without_tail
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        current.push_str(segment);
        current.push('/');
        let label = if first {
            parse_network_location(&current)
                .map(|location| location.display_name)
                .unwrap_or_else(|_| percent_decode_lossy(segment))
        } else {
            percent_decode_lossy(segment)
        };
        segments.push(BreadcrumbSegment {
            label,
            path: PathBuf::from(current.clone()),
        });
        first = false;
    }
    Some(segments)
}

fn percent_decode_lossy(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn split_location_input(input: &str) -> (&str, &str) {
    let input = input.trim();
    match input.rfind('/') {
        Some(0) => ("/", &input[1..]),
        Some(index) => (&input[..index], &input[index + 1..]),
        None => ("", input),
    }
}

fn join_location_text(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

pub fn expand_user_path(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::{
        breadcrumb_segments, complete_location_input, home_dir, parent_location,
        resolve_location_input,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fika-{name}-{unique}"))
    }

    #[test]
    fn location_input_resolves_absolute_relative_and_home_paths() {
        let current = Path::new("/tmp/fika-current");

        assert_eq!(
            resolve_location_input(current, "/etc"),
            Some(PathBuf::from("/etc"))
        );
        assert_eq!(
            resolve_location_input(current, "notes"),
            Some(PathBuf::from("/tmp/fika-current/notes"))
        );
        assert_eq!(resolve_location_input(current, "  "), None);
        assert_eq!(resolve_location_input(current, "~"), Some(home_dir()));
    }

    #[test]
    fn location_completion_uses_filesystem_and_sorts_matches() {
        let temp = test_dir("location-completion");
        std::fs::create_dir_all(temp.join("alpha")).unwrap();
        std::fs::write(temp.join("alpine.txt"), "file").unwrap();
        std::fs::create_dir_all(temp.join("nested")).unwrap();
        std::fs::create_dir_all(temp.join("nested/zed")).unwrap();
        std::fs::create_dir_all(temp.join("nested/zen")).unwrap();

        assert_eq!(
            complete_location_input(&temp, "al"),
            Some("alpha/".to_string())
        );
        assert_eq!(
            complete_location_input(&temp, "nested/ze"),
            Some("nested/zed/".to_string())
        );
        assert_eq!(complete_location_input(&temp, "missing"), None);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn breadcrumb_segments_build_incremental_paths() {
        let segments = breadcrumb_segments(Path::new("/home/yk/Documents"));
        let labels = segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect::<Vec<_>>();
        let paths = segments
            .iter()
            .map(|segment| segment.path.clone())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["/", "home", "yk", "Documents"]);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/"),
                PathBuf::from("/home"),
                PathBuf::from("/home/yk"),
                PathBuf::from("/home/yk/Documents"),
            ]
        );
    }

    #[test]
    fn network_location_input_and_breadcrumbs_preserve_uri_model() {
        let current = Path::new("smb://server/share/folder/");

        assert_eq!(
            resolve_location_input(Path::new("/tmp"), "smb://server/share/"),
            Some(PathBuf::from("smb://server/share/"))
        );
        assert_eq!(
            resolve_location_input(current, "../Other"),
            Some(PathBuf::from("smb://server/share/Other"))
        );
        assert_eq!(
            parent_location(current),
            Some(PathBuf::from("smb://server/share/"))
        );
        assert_eq!(complete_location_input(current, "rep"), None);

        let segments = breadcrumb_segments(Path::new("smb://server/share/folder/"));
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Network", "share on server", "folder"]
        );
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.path.clone())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("network:///"),
                PathBuf::from("smb://server/share/"),
                PathBuf::from("smb://server/share/folder/"),
            ]
        );
    }
}
