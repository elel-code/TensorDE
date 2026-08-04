use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

pub const MAX_FRAME_SIZE: usize = 1024 * 1024;
const FRAME_HEADER_SIZE: usize = std::mem::size_of::<u32>();

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let payload = serde_json::to_vec(value).map_err(CodecError::Serialize)?;
    let length = u32::try_from(payload.len()).map_err(|_| CodecError::FrameTooLarge)?;
    if payload.len() > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge);
    }

    let mut frame = Vec::with_capacity(FRAME_HEADER_SIZE + payload.len());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push<T: DeserializeOwned>(&mut self, bytes: &[u8]) -> Result<Vec<T>, CodecError> {
        self.buffer.extend_from_slice(bytes);
        let mut values = Vec::new();

        loop {
            if self.buffer.len() < FRAME_HEADER_SIZE {
                break;
            }

            let payload_len =
                u32::from_le_bytes(self.buffer[..FRAME_HEADER_SIZE].try_into().unwrap()) as usize;
            if payload_len > MAX_FRAME_SIZE {
                return Err(CodecError::FrameTooLarge);
            }

            let frame_len = FRAME_HEADER_SIZE + payload_len;
            if self.buffer.len() < frame_len {
                break;
            }

            let payload = &self.buffer[FRAME_HEADER_SIZE..frame_len];
            values.push(serde_json::from_slice(payload).map_err(CodecError::Deserialize)?);
            self.buffer.drain(..frame_len);
        }

        Ok(values)
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("IPC frame is larger than {MAX_FRAME_SIZE} bytes")]
    FrameTooLarge,
    #[error("failed to serialize IPC frame: {0}")]
    Serialize(serde_json::Error),
    #[error("failed to deserialize IPC frame: {0}")]
    Deserialize(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::super::message::{Command, Request};
    use super::*;

    #[test]
    fn fragmented_frames_are_reassembled() {
        let bytes = encode(&Request::new(7, Command::Ping)).unwrap();
        let split = bytes.len() / 2;
        let mut decoder = FrameDecoder::new();

        assert!(decoder.push::<Request>(&bytes[..split]).unwrap().is_empty());
        let messages = decoder.push::<Request>(&bytes[split..]).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].request_id, 7);
    }

    #[test]
    fn multiple_frames_share_one_read() {
        let mut bytes = encode(&Request::new(1, Command::Ping)).unwrap();
        bytes.extend(encode(&Request::new(2, Command::GetState)).unwrap());
        let messages = FrameDecoder::new().push::<Request>(&bytes).unwrap();

        assert_eq!(
            messages
                .iter()
                .map(|message| message.request_id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let length = u32::try_from(MAX_FRAME_SIZE + 1).unwrap().to_le_bytes();
        let error = FrameDecoder::new().push::<Request>(&length).unwrap_err();

        assert!(matches!(error, CodecError::FrameTooLarge));
    }
}
