use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

pub const MAX_FRAME_SIZE: usize = 1024 * 1024;
const FRAME_HEADER_SIZE: usize = std::mem::size_of::<u32>();

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let mut frame = Vec::new();
    encode_into(value, &mut frame)?;
    Ok(frame)
}

#[doc(hidden)]
pub fn encode_into<T: Serialize>(value: &T, frame: &mut Vec<u8>) -> Result<(), CodecError> {
    frame.clear();
    frame.resize(FRAME_HEADER_SIZE, 0);
    if let Err(error) = serde_json::to_writer(&mut *frame, value) {
        frame.clear();
        return Err(CodecError::Serialize(error));
    }
    let payload_len = frame.len() - FRAME_HEADER_SIZE;
    if payload_len > MAX_FRAME_SIZE {
        frame.clear();
        return Err(CodecError::FrameTooLarge);
    }
    let length = u32::try_from(payload_len).map_err(|_| CodecError::FrameTooLarge)?;
    frame[..FRAME_HEADER_SIZE].copy_from_slice(&length.to_le_bytes());
    Ok(())
}

pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push<T: DeserializeOwned>(&mut self, bytes: &[u8]) -> Result<Vec<T>, CodecError> {
        let mut values = Vec::new();
        self.push_into(bytes, &mut values)?;
        Ok(values)
    }

    #[doc(hidden)]
    pub fn push_into<T: DeserializeOwned>(
        &mut self,
        bytes: &[u8],
        values: &mut Vec<T>,
    ) -> Result<(), CodecError> {
        values.clear();
        self.buffer.extend_from_slice(bytes);
        let mut consumed = 0;

        loop {
            let remaining = &self.buffer[consumed..];
            if remaining.len() < FRAME_HEADER_SIZE {
                break;
            }

            let payload_len =
                u32::from_le_bytes(remaining[..FRAME_HEADER_SIZE].try_into().unwrap()) as usize;
            if payload_len > MAX_FRAME_SIZE {
                return Err(CodecError::FrameTooLarge);
            }

            let frame_len = FRAME_HEADER_SIZE + payload_len;
            if remaining.len() < frame_len {
                break;
            }

            let payload = &remaining[FRAME_HEADER_SIZE..frame_len];
            values.push(serde_json::from_slice(payload).map_err(CodecError::Deserialize)?);
            consumed += frame_len;
        }

        if consumed != 0 {
            self.buffer.drain(..consumed);
        }

        Ok(())
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
    fn completed_prefix_is_compacted_once_without_losing_the_fragmented_tail() {
        let mut bytes = Vec::new();
        for request_id in 1..=64 {
            bytes.extend(encode(&Request::new(request_id, Command::Ping)).unwrap());
        }
        let tail = encode(&Request::new(65, Command::GetState)).unwrap();
        let split = tail.len() / 2;
        bytes.extend_from_slice(&tail[..split]);

        let mut decoder = FrameDecoder::new();
        let completed = decoder.push::<Request>(&bytes).unwrap();

        assert_eq!(completed.len(), 64);
        assert_eq!(completed[0].request_id, 1);
        assert_eq!(completed[63].request_id, 64);
        assert_eq!(decoder.buffered_bytes(), split);

        let tail = decoder.push::<Request>(&tail[split..]).unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].request_id, 65);
        assert_eq!(decoder.buffered_bytes(), 0);
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let length = u32::try_from(MAX_FRAME_SIZE + 1).unwrap().to_le_bytes();
        let error = FrameDecoder::new().push::<Request>(&length).unwrap_err();

        assert!(matches!(error, CodecError::FrameTooLarge));
    }

    #[test]
    fn caller_owned_encode_and_decode_buffers_are_reused() {
        let mut frame = Vec::with_capacity(256);
        let frame_allocation = frame.as_ptr();
        encode_into(&Request::new(70, Command::Ping), &mut frame).unwrap();
        assert_eq!(frame.as_ptr(), frame_allocation);

        let mut decoder = FrameDecoder::new();
        let mut requests = Vec::<Request>::with_capacity(4);
        let request_allocation = requests.as_ptr();
        decoder.push_into(&frame, &mut requests).unwrap();

        assert_eq!(requests.as_ptr(), request_allocation);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].request_id, 70);
    }
}
