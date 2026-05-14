use std::io::Read;

use bytes::Bytes;
use flate2::read::GzDecoder;

use crate::error::{AiFoundationError, AiResult};
use crate::realtime::RealtimeEventId;

const PROTOCOL_VERSION: u8 = 0x1;
const HEADER_SIZE_WORDS: u8 = 0x1;
const HEADER_SIZE_BYTES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageType {
    FullClientRequest = 0x1,
    AudioOnlyRequest = 0x2,
    FullServerResponse = 0x9,
    AudioOnlyResponse = 0xB,
    ErrorInformation = 0xF,
}

impl MessageType {
    fn from_nibble(value: u8) -> AiResult<Self> {
        match value {
            0x1 => Ok(Self::FullClientRequest),
            0x2 => Ok(Self::AudioOnlyRequest),
            0x9 => Ok(Self::FullServerResponse),
            0xB => Ok(Self::AudioOnlyResponse),
            0xF => Ok(Self::ErrorInformation),
            _ => Err(AiFoundationError::Serialization(format!(
                "unknown realtime message type: {value:#x}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Serialization {
    Raw = 0x0,
    Json = 0x1,
}

impl Serialization {
    fn from_nibble(value: u8) -> AiResult<Self> {
        match value {
            0x0 => Ok(Self::Raw),
            0x1 => Ok(Self::Json),
            _ => Err(AiFoundationError::Serialization(format!(
                "unsupported realtime serialization method: {value:#x}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compression {
    None = 0x0,
    Gzip = 0x1,
}

impl Compression {
    fn from_nibble(value: u8) -> AiResult<Self> {
        match value {
            0x0 => Ok(Self::None),
            0x1 => Ok(Self::Gzip),
            _ => Err(AiFoundationError::Serialization(format!(
                "unsupported realtime compression method: {value:#x}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RealtimeFrame {
    pub message_type: MessageType,
    pub serialization: Serialization,
    pub compression: Compression,
    pub event_id: Option<u32>,
    pub code: Option<u32>,
    pub sequence: Option<i32>,
    pub connect_id: Option<String>,
    pub session_id: Option<String>,
    pub payload: Bytes,
}

impl RealtimeFrame {
    #[cfg(test)]
    pub fn json_client(
        event_id: RealtimeEventId,
        session_id: Option<String>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            message_type: MessageType::FullClientRequest,
            serialization: Serialization::Json,
            compression: Compression::None,
            event_id: Some(event_id.as_u32()),
            code: None,
            sequence: None,
            connect_id: None,
            session_id,
            payload: Bytes::from(payload),
        }
    }

    pub fn audio_client(event_id: RealtimeEventId, session_id: String, payload: Bytes) -> Self {
        Self {
            message_type: MessageType::AudioOnlyRequest,
            serialization: Serialization::Raw,
            compression: Compression::None,
            event_id: Some(event_id.as_u32()),
            code: None,
            sequence: None,
            connect_id: None,
            session_id: Some(session_id),
            payload,
        }
    }
}

pub(crate) fn encode_frame(frame: &RealtimeFrame) -> AiResult<Vec<u8>> {
    if frame.compression != Compression::None {
        return Err(AiFoundationError::InvalidRequest(
            "realtime frame encoding currently supports uncompressed payloads only".to_string(),
        ));
    }

    let mut flags = 0u8;
    if frame.code.is_some() {
        flags = 0xF;
    } else if frame.event_id.is_some() {
        flags = 0x4;
    } else if let Some(sequence) = frame.sequence {
        flags = if sequence < 0 { 0x3 } else { 0x1 };
    }

    let mut bytes = Vec::with_capacity(HEADER_SIZE_BYTES + 16 + frame.payload.len());
    bytes.push((PROTOCOL_VERSION << 4) | HEADER_SIZE_WORDS);
    bytes.push(((frame.message_type as u8) << 4) | flags);
    bytes.push(((frame.serialization as u8) << 4) | (frame.compression as u8));
    bytes.push(0x00);

    if let Some(code) = frame.code {
        write_u32(&mut bytes, code);
    }
    if matches!(flags, 0x1 | 0x3) {
        write_i32(&mut bytes, frame.sequence.unwrap_or_default());
    }
    if let Some(event_id) = frame.event_id {
        write_u32(&mut bytes, event_id);
    }
    if let Some(connect_id) = &frame.connect_id {
        write_sized_string(&mut bytes, connect_id)?;
    }
    if let Some(session_id) = &frame.session_id {
        write_sized_string(&mut bytes, session_id)?;
    }

    write_u32(&mut bytes, checked_len(frame.payload.len(), "payload")?);
    bytes.extend_from_slice(&frame.payload);
    Ok(bytes)
}

pub(crate) fn decode_frame(bytes: &[u8]) -> AiResult<RealtimeFrame> {
    if bytes.len() < HEADER_SIZE_BYTES {
        return Err(AiFoundationError::Serialization(
            "realtime frame shorter than 4-byte header".to_string(),
        ));
    }

    let version = bytes[0] >> 4;
    if version != PROTOCOL_VERSION {
        return Err(AiFoundationError::Serialization(format!(
            "unsupported realtime protocol version: {version}"
        )));
    }

    let header_size = ((bytes[0] & 0x0F) as usize) * 4;
    if header_size < HEADER_SIZE_BYTES || bytes.len() < header_size {
        return Err(AiFoundationError::Serialization(format!(
            "invalid realtime header size: {header_size}"
        )));
    }

    let message_type = MessageType::from_nibble(bytes[1] >> 4)?;
    let flags = bytes[1] & 0x0F;
    let serialization = Serialization::from_nibble(bytes[2] >> 4)?;
    let compression = Compression::from_nibble(bytes[2] & 0x0F)?;
    let mut cursor = header_size;

    let mut code = None;
    let mut sequence = None;
    let mut event_id = None;
    let mut connect_id = None;
    let mut session_id = None;

    match flags {
        0x0 => {}
        0x1 | 0x3 => {
            sequence = Some(read_i32(bytes, &mut cursor)?);
        }
        0x4 => {
            let event = read_u32(bytes, &mut cursor)?;
            event_id = Some(event);
            if should_have_session_id(event) {
                session_id = Some(read_sized_string(bytes, &mut cursor, "session id")?);
            } else if let Some(parsed) = try_read_connect_id(bytes, cursor) {
                connect_id = Some(parsed.0);
                cursor = parsed.1;
            }
        }
        0xF => {
            code = Some(read_u32(bytes, &mut cursor)?);
        }
        _ => {
            return Err(AiFoundationError::Serialization(format!(
                "unsupported realtime message flags: {flags:#x}"
            )));
        }
    }

    let payload_len = read_u32(bytes, &mut cursor)? as usize;
    let payload_end = cursor.checked_add(payload_len).ok_or_else(|| {
        AiFoundationError::Serialization("realtime payload length overflow".to_string())
    })?;
    if payload_end > bytes.len() {
        return Err(AiFoundationError::Serialization(format!(
            "realtime payload length {} exceeds remaining frame bytes {}",
            payload_len,
            bytes.len().saturating_sub(cursor)
        )));
    }

    let payload = match compression {
        Compression::None => Bytes::copy_from_slice(&bytes[cursor..payload_end]),
        Compression::Gzip => Bytes::from(decompress_gzip(&bytes[cursor..payload_end])?),
    };

    Ok(RealtimeFrame {
        message_type,
        serialization,
        compression,
        event_id,
        code,
        sequence,
        connect_id,
        session_id,
        payload,
    })
}

fn should_have_session_id(event_id: u32) -> bool {
    RealtimeEventId::from_u32(event_id)
        .map(RealtimeEventId::has_session_id)
        .unwrap_or(event_id >= 100)
}

fn try_read_connect_id(bytes: &[u8], cursor: usize) -> Option<(String, usize)> {
    if cursor + 4 > bytes.len() {
        return None;
    }
    let mut probe = cursor;
    let id_len = read_u32(bytes, &mut probe).ok()? as usize;
    if id_len == 0 || id_len > 128 || probe + id_len + 4 > bytes.len() {
        return None;
    }
    let id = std::str::from_utf8(&bytes[probe..probe + id_len]).ok()?;
    if !looks_like_id(id) {
        return None;
    }
    probe += id_len;
    let mut payload_probe = probe;
    let payload_len = read_u32(bytes, &mut payload_probe).ok()? as usize;
    if payload_probe + payload_len != bytes.len() {
        return None;
    }
    Some((id.to_string(), probe))
}

fn looks_like_id(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn write_sized_string(bytes: &mut Vec<u8>, value: &str) -> AiResult<()> {
    write_u32(bytes, checked_len(value.len(), "string")?);
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn checked_len(len: usize, name: &str) -> AiResult<u32> {
    u32::try_from(len)
        .map_err(|_| AiFoundationError::InvalidRequest(format!("realtime {name} is too large")))
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> AiResult<u32> {
    let value = bytes.get(*cursor..*cursor + 4).ok_or_else(|| {
        AiFoundationError::Serialization(
            "unexpected end of realtime frame while reading u32".to_string(),
        )
    })?;
    *cursor += 4;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_i32(bytes: &[u8], cursor: &mut usize) -> AiResult<i32> {
    let value = bytes.get(*cursor..*cursor + 4).ok_or_else(|| {
        AiFoundationError::Serialization(
            "unexpected end of realtime frame while reading i32".to_string(),
        )
    })?;
    *cursor += 4;
    Ok(i32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_sized_string(bytes: &[u8], cursor: &mut usize, field: &str) -> AiResult<String> {
    let len = read_u32(bytes, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or_else(|| {
        AiFoundationError::Serialization(format!("realtime {field} length overflow"))
    })?;
    let value = bytes.get(*cursor..end).ok_or_else(|| {
        AiFoundationError::Serialization(format!(
            "unexpected end of realtime frame while reading {field}"
        ))
    })?;
    *cursor = end;
    std::str::from_utf8(value)
        .map(ToString::to_string)
        .map_err(|error| {
            AiFoundationError::Serialization(format!("realtime {field} is not UTF-8: {error}"))
        })
}

fn decompress_gzip(bytes: &[u8]) -> AiResult<Vec<u8>> {
    let mut decoder = GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).map_err(|error| {
        AiFoundationError::Serialization(format!(
            "failed to decompress realtime gzip payload: {error}"
        ))
    })?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_start_connection_like_ark_doc_example() {
        let frame =
            RealtimeFrame::json_client(RealtimeEventId::StartConnection, None, b"{}".to_vec());
        assert_eq!(
            encode_frame(&frame).unwrap(),
            vec![17, 20, 16, 0, 0, 0, 0, 1, 0, 0, 0, 2, 123, 125]
        );
    }

    #[test]
    fn encodes_start_session_with_session_id() {
        let frame = RealtimeFrame::json_client(
            RealtimeEventId::StartSession,
            Some("75a6126e-427f-49a1-a2c1-621143cb9db3".to_string()),
            br#"{"dialog":{"bot_name":"bot","dialog_id":"","extra":null}}"#.to_vec(),
        );
        let encoded = encode_frame(&frame).unwrap();
        assert_eq!(&encoded[0..8], &[17, 20, 16, 0, 0, 0, 0, 100]);
        assert_eq!(&encoded[8..12], &[0, 0, 0, 36]);
    }

    #[test]
    fn decodes_audio_response_with_session_id() {
        let mut bytes = vec![17, 180, 0, 0, 0, 0, 1, 96, 0, 0, 0, 36];
        bytes.extend_from_slice(b"3c791a7d-227a-4446-993b-24f9e302cc98");
        bytes.extend_from_slice(&[0, 0, 0, 3, 1, 2, 3]);

        let frame = decode_frame(&bytes).unwrap();
        assert_eq!(frame.message_type, MessageType::AudioOnlyResponse);
        assert_eq!(frame.event_id, Some(352));
        assert_eq!(
            frame.session_id.as_deref(),
            Some("3c791a7d-227a-4446-993b-24f9e302cc98")
        );
        assert_eq!(frame.payload, Bytes::from_static(&[1, 2, 3]));
    }
}
