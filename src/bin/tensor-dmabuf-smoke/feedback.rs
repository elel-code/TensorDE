use std::{fs::File, io::Read, os::fd::OwnedFd};

use gbm::{Format as GbmFormat, Modifier};

use super::SmokeError;

const FORMAT_TABLE_ENTRY_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct DmabufFormat {
    pub(super) fourcc: u32,
    pub(super) modifier: u64,
}

#[derive(Debug, Default)]
pub(super) struct FeedbackState {
    table: Vec<DmabufFormat>,
    pending: FeedbackTranche,
    tranches: Vec<FeedbackTranche>,
    main_device: Option<libc::dev_t>,
    done: bool,
    error: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct FeedbackTranche {
    formats: Vec<u16>,
}

#[derive(Debug)]
pub(super) struct ReadyFeedback {
    pub(super) main_device: libc::dev_t,
    table: Vec<DmabufFormat>,
    tranches: Vec<FeedbackTranche>,
}

impl FeedbackState {
    pub(super) fn record_format_table(&mut self, fd: OwnedFd, size: u32) {
        let result = read_format_table(fd, size).map(|table| self.table = table);
        if let Err(error) = result {
            self.fail(error);
        }
    }

    pub(super) fn record_main_device(&mut self, bytes: &[u8]) {
        match parse_device(bytes) {
            Ok(device) => self.main_device = Some(device),
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn record_indices(&mut self, bytes: &[u8]) {
        match parse_indices(bytes) {
            Ok(indices) => self.pending.formats = indices,
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn finish_tranche(&mut self) {
        self.tranches.push(std::mem::take(&mut self.pending));
    }

    pub(super) fn mark_done(&mut self) {
        self.done = true;
    }

    pub(super) fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub(super) fn take_ready(&mut self) -> Result<ReadyFeedback, SmokeError> {
        if let Some(error) = self.error.take() {
            return Err(SmokeError::InvalidFeedback(error));
        }
        if !self.done {
            return Err(SmokeError::InvalidFeedback(
                "default dma-buf feedback did not send done".to_owned(),
            ));
        }
        let main_device = self.main_device.ok_or_else(|| {
            SmokeError::InvalidFeedback("default dma-buf feedback omitted main_device".to_owned())
        })?;
        if self.table.is_empty() {
            return Err(SmokeError::InvalidFeedback(
                "default dma-buf feedback supplied an empty format table".to_owned(),
            ));
        }
        Ok(ReadyFeedback {
            main_device,
            table: std::mem::take(&mut self.table),
            tranches: std::mem::take(&mut self.tranches),
        })
    }

    fn fail(&mut self, error: impl Into<String>) {
        self.error.get_or_insert_with(|| error.into());
    }
}

impl ReadyFeedback {
    pub(super) fn preferred_formats(&self) -> Result<Vec<DmabufFormat>, SmokeError> {
        let mut formats = Vec::new();
        for tranche in &self.tranches {
            for index in &tranche.formats {
                let Some(format) = self.table.get(usize::from(*index)).copied() else {
                    return Err(SmokeError::InvalidFeedback(format!(
                        "tranche referenced missing format-table index {index}"
                    )));
                };
                if format.modifier != u64::from(Modifier::Invalid) && !formats.contains(&format) {
                    formats.push(format);
                }
            }
        }
        formats.sort_by_key(|format| {
            (
                fourcc_preference(format.fourcc),
                u8::from(format.modifier == u64::from(Modifier::Linear)),
                format.modifier,
            )
        });
        if formats.is_empty() {
            return Err(SmokeError::InvalidFeedback(
                "default dma-buf feedback has no explicit format/modifier pair".to_owned(),
            ));
        }
        Ok(formats)
    }
}

fn read_format_table(fd: OwnedFd, size: u32) -> Result<Vec<DmabufFormat>, String> {
    let size = usize::try_from(size).map_err(|_| "format table size overflows usize".to_owned())?;
    if size == 0 || size % FORMAT_TABLE_ENTRY_SIZE != 0 {
        return Err(format!(
            "format table size {size} is not a non-zero multiple of 16"
        ));
    }
    let mut bytes = vec![0; size];
    File::from(fd)
        .read_exact(&mut bytes)
        .map_err(|error| format!("failed to read format table: {error}"))?;
    Ok(bytes
        .chunks_exact(FORMAT_TABLE_ENTRY_SIZE)
        .map(|entry| DmabufFormat {
            fourcc: u32::from_ne_bytes(entry[0..4].try_into().expect("entry length is checked")),
            modifier: u64::from_ne_bytes(entry[8..16].try_into().expect("entry length is checked")),
        })
        .collect())
}

fn parse_device(bytes: &[u8]) -> Result<libc::dev_t, String> {
    let expected = std::mem::size_of::<libc::dev_t>();
    let raw: [u8; std::mem::size_of::<libc::dev_t>()] = bytes.try_into().map_err(|_| {
        format!(
            "DRM device array has {} bytes, expected {expected}",
            bytes.len()
        )
    })?;
    Ok(libc::dev_t::from_ne_bytes(raw))
}

fn parse_indices(bytes: &[u8]) -> Result<Vec<u16>, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err(format!(
            "format-table indices have odd byte length {}",
            bytes.len()
        ));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|entry| u16::from_ne_bytes([entry[0], entry[1]]))
        .collect())
}

fn fourcc_preference(fourcc: u32) -> u8 {
    match GbmFormat::try_from(fourcc) {
        Ok(GbmFormat::Xrgb8888) => 0,
        Ok(GbmFormat::Argb8888) => 1,
        Ok(GbmFormat::Xbgr8888) => 2,
        Ok(GbmFormat::Abgr8888) => 3,
        Ok(GbmFormat::Xrgb2101010) => 4,
        Ok(GbmFormat::Argb2101010) => 5,
        Ok(GbmFormat::Xbgr2101010) => 6,
        Ok(GbmFormat::Abgr2101010) => 7,
        _ => u8::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_table_uses_native_layout_and_rejects_partial_entries() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x3432_5241_u32.to_ne_bytes());
        bytes.extend_from_slice(&0_u32.to_ne_bytes());
        bytes.extend_from_slice(&0x0102_0304_0506_0708_u64.to_ne_bytes());

        let parsed = bytes
            .chunks_exact(FORMAT_TABLE_ENTRY_SIZE)
            .map(|entry| DmabufFormat {
                fourcc: u32::from_ne_bytes(entry[0..4].try_into().unwrap()),
                modifier: u64::from_ne_bytes(entry[8..16].try_into().unwrap()),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parsed,
            vec![DmabufFormat {
                fourcc: 0x3432_5241,
                modifier: 0x0102_0304_0506_0708,
            }]
        );
        assert!(bytes.len().is_multiple_of(FORMAT_TABLE_ENTRY_SIZE));
        assert!(parse_indices(&[0]).is_err());
    }

    #[test]
    fn preferred_formats_drop_implicit_modifiers_and_keep_tranche_order() {
        let explicit = DmabufFormat {
            fourcc: GbmFormat::Argb8888 as u32,
            modifier: 9,
        };
        let feedback = ReadyFeedback {
            main_device: 1,
            table: vec![
                DmabufFormat {
                    fourcc: GbmFormat::Argb8888 as u32,
                    modifier: u64::from(Modifier::Invalid),
                },
                explicit,
            ],
            tranches: vec![FeedbackTranche {
                formats: vec![0, 1, 1],
            }],
        };

        assert_eq!(feedback.preferred_formats().unwrap(), vec![explicit]);
    }
}
