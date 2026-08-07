use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const MAX_GREETD_FRAME_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Request<'a> {
    CreateSession {
        username: &'a str,
    },
    PostAuthMessageResponse {
        response: Option<&'a str>,
    },
    StartSession {
        cmd: &'a [String],
        env: &'a [String],
    },
    CancelSession,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Response {
    Success,
    Error {
        error_type: ErrorType,
        description: String,
    },
    AuthMessage {
        auth_message_type: AuthMessageType,
        auth_message: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    AuthError,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMessageType {
    Visible,
    Secret,
    Info,
    Error,
}

/// A greetd request frame whose bytes are overwritten when dropped.
///
/// This type intentionally has no `Debug`, `Clone`, or consuming byte accessor.
pub struct SensitiveFrame {
    bytes: Vec<u8>,
}

impl SensitiveFrame {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl compio::buf::IoBuf for SensitiveFrame {
    fn as_init(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for SensitiveFrame {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

pub fn encode_request(request: &Request<'_>) -> Result<SensitiveFrame, GreetdProtocolError> {
    let payload = serde_json::to_vec(request)?;
    if payload.len() > MAX_GREETD_FRAME_BYTES {
        return Err(GreetdProtocolError::FrameTooLarge {
            bytes: payload.len(),
            maximum: MAX_GREETD_FRAME_BYTES,
        });
    }
    let length = u32::try_from(payload.len()).expect("greetd frame limit fits in u32");
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(SensitiveFrame { bytes })
}

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<T: DeserializeOwned>(
        &mut self,
        bytes: &[u8],
    ) -> Result<Vec<T>, GreetdProtocolError> {
        if self.buffer.len().saturating_add(bytes.len()) > MAX_GREETD_FRAME_BYTES + 4 {
            self.buffer.clear();
            return Err(GreetdProtocolError::BufferedDataTooLarge);
        }
        self.buffer.extend_from_slice(bytes);
        let mut decoded = Vec::new();
        loop {
            if self.buffer.len() < 4 {
                break;
            }
            let length = u32::from_le_bytes(self.buffer[..4].try_into().unwrap()) as usize;
            if length > MAX_GREETD_FRAME_BYTES {
                self.buffer.clear();
                return Err(GreetdProtocolError::FrameTooLarge {
                    bytes: length,
                    maximum: MAX_GREETD_FRAME_BYTES,
                });
            }
            let frame_end = 4 + length;
            if self.buffer.len() < frame_end {
                break;
            }
            let message = serde_json::from_slice(&self.buffer[4..frame_end])?;
            decoded.push(message);
            self.buffer.drain(..frame_end);
        }
        Ok(decoded)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GreetdProtocolError {
    #[error("invalid greetd JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("greetd frame has {bytes} bytes; maximum is {maximum}")]
    FrameTooLarge { bytes: usize, maximum: usize },
    #[error("buffered greetd data exceeds one bounded frame")]
    BufferedDataTooLarge,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_greetd_little_endian_json_framing() {
        let frame = encode_request(&Request::CreateSession { username: "tensor" }).unwrap();
        let length = u32::from_le_bytes(frame.as_bytes()[..4].try_into().unwrap()) as usize;
        assert_eq!(length, frame.as_bytes().len() - 4);
        assert_eq!(
            &frame.as_bytes()[4..],
            br#"{"type":"create_session","username":"tensor"}"#
        );
    }

    #[test]
    fn decoder_handles_fragmentation_and_multiple_frames() {
        let success = br#"{"type":"success"}"#;
        let prompt =
            br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password:"}"#;
        let mut wire = Vec::new();
        wire.extend_from_slice(&(success.len() as u32).to_le_bytes());
        wire.extend_from_slice(success);
        wire.extend_from_slice(&(prompt.len() as u32).to_le_bytes());
        wire.extend_from_slice(prompt);
        let mut decoder = FrameDecoder::new();
        assert!(decoder.push::<Response>(&wire[..3]).unwrap().is_empty());
        let decoded = decoder.push::<Response>(&wire[3..]).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], Response::Success);
        assert!(matches!(
            &decoded[1],
            Response::AuthMessage {
                auth_message_type: AuthMessageType::Secret,
                ..
            }
        ));
    }

    #[test]
    fn oversized_length_is_rejected_before_payload_arrives() {
        let mut decoder = FrameDecoder::new();
        let length = (MAX_GREETD_FRAME_BYTES as u32 + 1).to_le_bytes();
        assert!(matches!(
            decoder.push::<Response>(&length),
            Err(GreetdProtocolError::FrameTooLarge { .. })
        ));
    }
}
