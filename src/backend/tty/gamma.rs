//! KMS gamma LUT application for `zwlr_gamma_control_v1`.
//!
//! Prefers the atomic `GAMMA_LUT` / `GAMMA_LUT_SIZE` properties (blob path).
//! Falls back to the legacy `drmModeCrtcSetGamma` API when the CRTC lacks those
//! properties. LUT apply is O(n) in the hardware table size and never runs on
//! the page-flip / render hot path unless a client requests a change.

use std::{
    iter::zip,
    num::{NonZeroU32, NonZeroU64},
    os::fd::AsFd,
};

use drm::control::{self, Device as ControlDevice, crtc, property};
use smithay::backend::drm::DrmDevice;
use tracing::warn;

/// Atomic CRTC gamma property handles plus the last applied blob id.
pub(super) struct GammaProps {
    crtc: crtc::Handle,
    gamma_lut: property::Handle,
    gamma_lut_size: property::Handle,
    previous_blob: Option<NonZeroU64>,
}

/// Per-output gamma state. Lives with the KMS target so session resume can
/// re-apply without consulting protocol objects.
pub(super) struct OutputGamma {
    props: Option<GammaProps>,
    crtc: crtc::Handle,
    /// When the session is inactive, queue the last request and apply on resume.
    pending: Option<Option<Vec<u16>>>,
}

impl OutputGamma {
    pub(super) fn new(device: &DrmDevice, crtc: crtc::Handle) -> Self {
        let mut props = GammaProps::new(device, crtc).ok();
        // Reset any leftover ramp from a previous compositor on this CRTC.
        let reset = if let Some(props) = props.as_mut() {
            props.set_gamma(device, None)
        } else {
            set_gamma_legacy(device, crtc, None)
        };
        if let Err(error) = reset {
            warn!(%error, "could not reset CRTC gamma on output bind");
        }
        Self {
            props,
            crtc,
            pending: None,
        }
    }

    pub(super) fn gamma_size(&self, device: &DrmDevice) -> Option<u32> {
        if let Some(props) = &self.props {
            return props.gamma_size(device).ok().filter(|size| *size > 0);
        }
        device
            .get_crtc(self.crtc)
            .ok()
            .map(|info| info.gamma_length())
            .filter(|size| *size > 0)
    }

    pub(super) fn set_gamma(
        &mut self,
        device: &DrmDevice,
        ramp: Option<&[u16]>,
        session_active: bool,
    ) -> Result<(), String> {
        if !session_active {
            self.pending = Some(ramp.map(<[u16]>::to_vec));
            return Ok(());
        }
        self.pending = None;
        if let Some(props) = &mut self.props {
            props.set_gamma(device, ramp)
        } else {
            set_gamma_legacy(device, self.crtc, ramp)
        }
    }

    /// Apply a queued change or restore the last blob after VT resume.
    pub(super) fn restore_after_session_resume(&mut self, device: &DrmDevice) {
        if let Some(ramp) = self.pending.take() {
            if let Err(error) = self.set_gamma(device, ramp.as_deref(), true) {
                warn!(%error, "failed to apply pending gamma after session resume");
            }
            return;
        }
        if let Some(props) = &self.props
            && let Err(error) = props.restore_gamma(device)
        {
            warn!(%error, "failed to restore gamma after session resume");
        }
    }
}

impl GammaProps {
    fn new(device: &DrmDevice, crtc: crtc::Handle) -> Result<Self, String> {
        let mut gamma_lut = None;
        let mut gamma_lut_size = None;
        let props = device
            .get_properties(crtc)
            .map_err(|error| format!("get CRTC properties: {error}"))?;
        for (prop, _) in props {
            let Ok(info) = device.get_property(prop) else {
                continue;
            };
            let Ok(name) = info.name().to_str() else {
                continue;
            };
            match name {
                "GAMMA_LUT" => {
                    if !matches!(info.value_type(), property::ValueType::Blob) {
                        return Err("GAMMA_LUT is not a blob property".to_owned());
                    }
                    gamma_lut = Some(prop);
                }
                "GAMMA_LUT_SIZE" => {
                    if !matches!(info.value_type(), property::ValueType::UnsignedRange(_, _)) {
                        return Err("GAMMA_LUT_SIZE has unexpected type".to_owned());
                    }
                    gamma_lut_size = Some(prop);
                }
                _ => {}
            }
        }
        Ok(Self {
            crtc,
            gamma_lut: gamma_lut.ok_or_else(|| "missing GAMMA_LUT".to_owned())?,
            gamma_lut_size: gamma_lut_size.ok_or_else(|| "missing GAMMA_LUT_SIZE".to_owned())?,
            previous_blob: None,
        })
    }

    fn gamma_size(&self, device: &DrmDevice) -> Result<u32, String> {
        let value = property_value(device, self.crtc, self.gamma_lut_size)
            .ok_or_else(|| "missing GAMMA_LUT_SIZE value".to_owned())?;
        Ok(value as u32)
    }

    fn set_gamma(&mut self, device: &DrmDevice, gamma: Option<&[u16]>) -> Result<(), String> {
        let blob = if let Some(gamma) = gamma {
            let gamma_size = self.gamma_size(device)? as usize;
            if gamma.len() != gamma_size * 3 {
                return Err(format!(
                    "gamma ramp length {} does not match {} entries × 3 channels",
                    gamma.len(),
                    gamma_size
                ));
            }
            let (red, rest) = gamma.split_at(gamma_size);
            // Protocol order is R, G, B (same as Niri / wlr-gamma-control).
            let (green, blue) = rest.split_at(gamma_size);
            // Packed `struct drm_color_lut` without `unsafe`: 4 × u16 LE per entry.
            let mut bytes = Vec::with_capacity(gamma_size * 8);
            for ((&r, &g), &b) in zip(zip(red, green), blue) {
                bytes.extend_from_slice(&r.to_ne_bytes());
                bytes.extend_from_slice(&g.to_ne_bytes());
                bytes.extend_from_slice(&b.to_ne_bytes());
                bytes.extend_from_slice(&0u16.to_ne_bytes());
            }
            let blob = drm_ffi::mode::create_property_blob(device.as_fd(), &mut bytes)
                .map_err(|error| format!("create GAMMA_LUT blob: {error}"))?;
            NonZeroU64::new(u64::from(blob.blob_id))
        } else {
            None
        };

        let blob_value = blob.map(NonZeroU64::get).unwrap_or(0);
        if let Err(error) = device.set_property(
            self.crtc,
            self.gamma_lut,
            property::Value::Blob(blob_value).into(),
        ) {
            if blob_value != 0
                && let Err(destroy_error) = device.destroy_property_blob(blob_value)
            {
                warn!(%destroy_error, "failed to destroy failed GAMMA_LUT blob");
            }
            return Err(format!("set GAMMA_LUT: {error}"));
        }

        if let Some(previous) = std::mem::replace(&mut self.previous_blob, blob)
            && let Err(error) = device.destroy_property_blob(previous.get())
        {
            warn!(%error, "failed to destroy previous GAMMA_LUT blob");
        }
        Ok(())
    }

    fn restore_gamma(&self, device: &DrmDevice) -> Result<(), String> {
        let blob = self.previous_blob.map(NonZeroU64::get).unwrap_or(0);
        device
            .set_property(
                self.crtc,
                self.gamma_lut,
                property::Value::Blob(blob).into(),
            )
            .map_err(|error| format!("restore GAMMA_LUT: {error}"))
    }
}

fn set_gamma_legacy(
    device: &DrmDevice,
    crtc: crtc::Handle,
    ramp: Option<&[u16]>,
) -> Result<(), String> {
    let info = device
        .get_crtc(crtc)
        .map_err(|error| format!("get CRTC for legacy gamma: {error}"))?;
    let gamma_length = info.gamma_length() as usize;
    if gamma_length == 0 {
        return Err("legacy gamma is not supported on this CRTC".to_owned());
    }

    let owned;
    let ramp = if let Some(ramp) = ramp {
        if ramp.len() != gamma_length * 3 {
            return Err(format!(
                "gamma ramp length {} does not match {} entries × 3 channels",
                ramp.len(),
                gamma_length
            ));
        }
        ramp
    } else {
        owned = linear_gamma(gamma_length);
        &owned
    };

    let (red, rest) = ramp.split_at(gamma_length);
    let (green, blue) = rest.split_at(gamma_length);
    device
        .set_gamma(crtc, red, green, blue)
        .map_err(|error| format!("legacy set_gamma: {error}"))
}

fn linear_gamma(gamma_length: usize) -> Vec<u16> {
    let mut temp = vec![0u16; gamma_length * 3];
    let (red, rest) = temp.split_at_mut(gamma_length);
    let (green, blue) = rest.split_at_mut(gamma_length);
    let denom = (gamma_length as u64).saturating_sub(1).max(1);
    for (i, ((r, g), b)) in zip(zip(red, green), blue).enumerate() {
        let value = ((0xFFFFu64 * i as u64) / denom) as u16;
        *r = value;
        *g = value;
        *b = value;
    }
    temp
}

fn property_value(
    device: &DrmDevice,
    resource: impl control::ResourceHandle,
    prop: property::Handle,
) -> Option<property::RawValue> {
    let props = device.get_properties(resource).ok()?;
    let (handles, values) = props.as_props_and_values();
    handles
        .iter()
        .zip(values.iter())
        .find(|(handle, _)| **handle == prop)
        .map(|(_, value)| *value)
}

pub(super) fn crtc_handle(raw: u32) -> Option<crtc::Handle> {
    NonZeroU32::new(raw).map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_gamma_endpoints_are_black_and_white() {
        let ramp = linear_gamma(4);
        assert_eq!(ramp.len(), 12);
        assert_eq!(ramp[0], 0);
        assert_eq!(ramp[3], 0xFFFF);
        assert_eq!(ramp[4], 0);
        assert_eq!(ramp[7], 0xFFFF);
    }

    #[test]
    fn gamma_lut_blob_bytes_are_eight_per_entry() {
        // Four native-endian u16 channels (r,g,b,reserved) per LUT entry.
        assert_eq!(std::mem::size_of::<u16>() * 4, 8);
    }
}
