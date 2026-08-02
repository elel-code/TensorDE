//! Allocation-free writes into retained per-frame scene buffers.

use super::*;

impl SharedSceneFrameResources {
    pub(in super::super) fn write_transform_payload(&self, payload: &[u8]) -> Result<(), String> {
        write_exact(&self.transform, "transform", payload)
    }

    pub(in super::super) fn write_video_vertex_payload(&self, payload: &[u8]) -> Result<(), String> {
        let buffer = self
            .video_vertex
            .as_ref()
            .ok_or_else(|| "scene video payload has no retained frame buffer".to_owned())?;
        write_exact(buffer, "video vertex", payload)
    }

    pub(in super::super) fn write_material_payload(&self, payload: &[u8]) -> Result<(), String> {
        let buffer = self
            .material
            .as_ref()
            .ok_or_else(|| "scene material payload has no retained frame buffer".to_owned())?;
        write_exact(buffer, "material", payload)
    }

    pub(in super::super) fn write_skinning_payload(&self, payload: &[u8]) -> Result<(), String> {
        let buffer = self
            .skinning
            .as_ref()
            .ok_or_else(|| "scene skinning payload has no retained frame buffer".to_owned())?;
        write_exact(buffer, "skinning", payload)
    }

    pub(in super::super) fn write_scene_owned_uniform_payload(
        &self,
        payload: &[u8],
    ) -> Result<(), String> {
        let buffer = self
            .scene_owned_uniform
            .as_ref()
            .ok_or_else(|| "scene-owned uniform payload has no retained frame buffer".to_owned())?;
        write_exact(buffer, "scene-owned uniform", payload)
    }

    pub(super) fn write_payloads(
        &self,
        payloads: SharedSceneFramePayloads<'_>,
    ) -> Result<(), String> {
        write_exact(&self.transform, "transform", payloads.transform)?;
        write_optional_exact(&self.video_vertex, "video vertex", payloads.video_vertex)?;
        write_optional_exact(&self.material, "material", payloads.material)?;
        write_optional_exact(&self.skinning, "skinning", payloads.skinning)?;
        write_optional_exact(
            &self.scene_owned_uniform,
            "scene-owned uniform",
            payloads.scene_owned_uniform,
        )
    }
}

fn write_optional_exact(
    buffer: &Option<Buffer>,
    label: &str,
    payload: Option<&[u8]>,
) -> Result<(), String> {
    match (buffer, payload) {
        (Some(buffer), Some(payload)) => write_exact(buffer, label, payload),
        (None, None) => Ok(()),
        (Some(_), None) => Err(format!(
            "retained scene {label} buffer has no frame payload"
        )),
        (None, Some(_)) => Err(format!(
            "scene {label} payload has no retained frame buffer"
        )),
    }
}

fn write_exact(buffer: &Buffer, label: &str, payload: &[u8]) -> Result<(), String> {
    if payload.len() as u64 != buffer.size() {
        return Err(format!(
            "scene {label} frame payload has {} bytes but retained buffer has {} bytes",
            payload.len(),
            buffer.size()
        ));
    }
    unsafe { buffer.write(0, payload) }
        .map_err(|error| format!("write retained scene {label} buffer: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_frame_payload_presence_must_match_retained_resources() {
        assert!(write_optional_exact(&None, "material", None).is_ok());
        assert!(write_optional_exact(&None, "material", Some(&[0; 4])).is_err());
    }
}
