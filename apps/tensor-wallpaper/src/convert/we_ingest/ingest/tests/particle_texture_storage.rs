use std::fs;

use super::super::*;
use crate::engine::scene::SceneTextureFormat;

#[test]
fn decoded_rgba_particle_texture_stays_lossless() {
    let root = std::env::temp_dir().join(format!(
        "tensor-wallpaper-particle-rgba-storage-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("particles")).expect("particles");
    fs::create_dir_all(root.join("materials/particle")).expect("particle materials");
    fs::write(
        root.join("project.json"),
        r#"{"type":"scene","file":"scene.json","title":"RGBA particle"}"#,
    )
    .expect("project");
    fs::write(
        root.join("scene.json"),
        r#"{"general":{"orthogonalprojection":{"width":4,"height":4}},"objects":[{"id":1,"name":"particle","particle":"particles/particle.json"}]}"#,
    )
    .expect("scene");
    fs::write(
        root.join("particles/particle.json"),
        r#"{"material":"materials/particle/material.json","maxcount":1,"emitter":[{"id":1,"name":"boxrandom","rate":1}],"initializer":[{"id":2,"name":"lifetimerandom","min":1,"max":1},{"id":3,"name":"sizerandom","min":1,"max":1}],"operator":[],"renderer":[{"id":4,"name":"sprite"}]}"#,
    )
    .expect("particle");
    fs::write(
        root.join("materials/particle/material.json"),
        r#"{"passes":[{"shader":"genericparticle","textures":["sprite.tex"]}]}"#,
    )
    .expect("material");
    let pixels = (0u8..64).collect::<Vec<_>>();
    fs::write(
        root.join("materials/particle/sprite.tex"),
        rgba_texb0004(4, 4, &pixels),
    )
    .expect("texture");

    let ir = ingest_wallpaper_engine_project(&root).expect("particle IR");
    let texture = ir
        .textures
        .iter()
        .find(|texture| {
            ir.resources[texture.resource as usize]
                .path
                .ends_with("materials/particle/sprite.tex")
        })
        .expect("particle texture");

    assert_eq!(texture.format, SceneTextureFormat::Rgba8Unorm);
    assert_eq!(texture.upload_payload, pixels);
    assert_eq!(texture.mips[0].payload_len, 64);
    let _ = fs::remove_dir_all(root);
}

fn rgba_texb0004(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TEXV0005\0TEXI0001\0");
    for value in [0, 4, width, height, width, height] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&[1, 0xff]);
    bytes.extend_from_slice(b"TEXB0004\0");
    for value in [
        1,
        u32::MAX,
        0,
        1,
        width,
        height,
        0,
        pixels.len() as u32,
        pixels.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(pixels);
    bytes
}
