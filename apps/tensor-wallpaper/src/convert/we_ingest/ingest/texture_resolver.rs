//! Material-aware WE texture path resolution.

use std::path::Path;

pub(super) fn texture_candidates(path: &str, material_path: Option<&str>) -> Vec<String> {
    let path = normalize(path);
    let mut bases = vec![path.clone()];
    if !path.starts_with("materials/") {
        bases.push(format!("materials/{path}"));
    }
    if let Some(parent) = material_path.and_then(|path| Path::new(path).parent()) {
        let relative = normalize(&parent.join(&path).to_string_lossy());
        if !relative.is_empty() {
            bases.push(relative);
        }
    }

    let mut candidates = Vec::new();
    for base in bases {
        if Path::new(&base).extension().is_some() {
            push_unique(&mut candidates, base);
        } else {
            for extension in ["tex", "png", "jpg", "jpeg", "webp"] {
                push_unique(&mut candidates, format!("{base}.{extension}"));
            }
        }
    }
    candidates
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bare_texture_under_materials() {
        let candidates = texture_candidates("background", Some("materials/background.json"));
        assert_eq!(candidates[0], "background.tex");
        assert!(candidates.contains(&"materials/background.tex".to_owned()));
    }

    #[test]
    fn resolves_effect_texture_from_global_material_root() {
        let candidates = texture_candidates(
            "effects/waterripplenormal",
            Some("materials/effects/waterripple.json"),
        );
        assert!(candidates.contains(&"materials/effects/waterripplenormal.tex".to_owned()));
    }

    #[test]
    fn keeps_material_relative_candidate_without_duplicates() {
        let candidates = texture_candidates("mask", Some("materials/custom/layer.json"));
        assert!(candidates.contains(&"materials/custom/mask.tex".to_owned()));
        let unique = candidates.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), candidates.len());
    }
}
