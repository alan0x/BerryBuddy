use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderMap, HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use uuid::Uuid;

use crate::error::{AiFoundationError, AiResult};
use crate::operation::Provider;
use crate::providers::ark::realtime_protocol::{
    MessageType, RealtimeFrame, Serialization, decode_frame, encode_frame,
};
use crate::realtime::{
    RealtimeAsrResult, RealtimeClientEvent, RealtimeConnectRequest, RealtimeConversationProvider,
    RealtimeConversationSession, RealtimeEventId, RealtimeServerEvent, RealtimeUsage,
};

const DEFAULT_REALTIME_ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/realtime/dialogue";
const DEFAULT_RESOURCE_ID: &str = "volc.speech.dialog";
const DEFAULT_APP_KEY: &str = "PlgvMymc7f3tQnJ6";

#[derive(Debug, Clone)]
pub struct ArkRealtimeConfig {
    pub app_id: String,
    pub access_key: String,
    pub resource_id: String,
    pub app_key: String,
    pub endpoint: String,
}

impl ArkRealtimeConfig {
    pub fn new(app_id: impl Into<String>, access_key: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            access_key: access_key.into(),
            resource_id: DEFAULT_RESOURCE_ID.to_string(),
            app_key: DEFAULT_APP_KEY.to_string(),
            endpoint: DEFAULT_REALTIME_ENDPOINT.to_string(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_resource_id(mut self, resource_id: impl Into<String>) -> Self {
        self.resource_id = resource_id.into();
        self
    }

    pub fn with_app_key(mut self, app_key: impl Into<String>) -> Self {
        self.app_key = app_key.into();
        self
    }
}

#[derive(Debug, Clone)]
pub struct ArkRealtimeProvider {
    config: ArkRealtimeConfig,
}

impl ArkRealtimeProvider {
    pub fn new(config: ArkRealtimeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl RealtimeConversationProvider for ArkRealtimeProvider {
    async fn connect_realtime_conversation(
        &self,
        request: RealtimeConnectRequest,
    ) -> AiResult<Arc<dyn RealtimeConversationSession>> {
        let connect_id = request
            .connect_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let ws_request = build_ws_request(&self.config, &connect_id)?;
        let (ws, _) = connect_async(ws_request).await.map_err(|error| {
            AiFoundationError::Network(format!("Ark realtime websocket connect failed: {error}"))
        })?;

        let (writer, reader) = ws.split();
        let session = Arc::new(ArkRealtimeSession {
            connect_id,
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        });
        if request.auto_start_connection {
            session
                .send_client_event(RealtimeClientEvent::StartConnection {
                    payload: request.start_connection_payload,
                })
                .await?;
        }

        Ok(session)
    }
}

type ArkRealtimeWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct ArkRealtimeSession {
    connect_id: String,
    writer: Mutex<SplitSink<ArkRealtimeWebSocket, Message>>,
    reader: Mutex<SplitStream<ArkRealtimeWebSocket>>,
}

impl ArkRealtimeSession {
    pub fn new_session_id() -> String {
        Uuid::new_v4().to_string()
    }
}

#[async_trait]
impl RealtimeConversationSession for ArkRealtimeSession {
    fn provider(&self) -> Provider {
        Provider::Ark
    }

    fn connect_id(&self) -> &str {
        &self.connect_id
    }

    async fn send_client_event(&self, event: RealtimeClientEvent) -> AiResult<()> {
        let frame = client_event_to_frame(event)?;
        let bytes = encode_frame(&frame)?;
        self.writer
            .lock()
            .await
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(|error| {
                AiFoundationError::Network(format!("Ark realtime websocket send failed: {error}"))
            })
    }

    async fn next_event(&self) -> AiResult<Option<RealtimeServerEvent>> {
        let mut reader = self.reader.lock().await;
        loop {
            let Some(message) = reader.next().await else {
                return Ok(None);
            };

            match message.map_err(|error| {
                AiFoundationError::Network(format!(
                    "Ark realtime websocket receive failed: {error}"
                ))
            })? {
                Message::Binary(bytes) => {
                    let frame = decode_frame(&bytes)?;
                    return frame_to_server_event(frame).map(Some);
                }
                Message::Close(_) => return Ok(None),
                Message::Ping(payload) => {
                    self.writer
                        .lock()
                        .await
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| {
                            AiFoundationError::Network(format!(
                                "Ark realtime websocket pong failed: {error}"
                            ))
                        })?;
                }
                Message::Text(text) => {
                    return Ok(Some(RealtimeServerEvent::Raw {
                        event_id: None,
                        session_id: None,
                        payload: Bytes::from(text.to_string()),
                    }));
                }
                Message::Pong(_) | Message::Frame(_) => {}
            }
        }
    }

    async fn close(&self) -> AiResult<()> {
        let _ = self
            .send_client_event(RealtimeClientEvent::FinishConnection)
            .await;
        self.writer
            .lock()
            .await
            .send(Message::Close(None))
            .await
            .map_err(|error| {
                AiFoundationError::Network(format!("Ark realtime websocket close failed: {error}"))
            })
    }
}

fn build_ws_request(
    config: &ArkRealtimeConfig,
    connect_id: &str,
) -> AiResult<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let mut request = config
        .endpoint
        .as_str()
        .into_client_request()
        .map_err(|error| {
            AiFoundationError::InvalidRequest(format!("invalid Ark realtime endpoint: {error}"))
        })?;
    let headers = request.headers_mut();
    insert_header(headers, "X-Api-App-ID", &config.app_id)?;
    insert_header(headers, "X-Api-Access-Key", &config.access_key)?;
    insert_header(headers, "X-Api-Resource-Id", &config.resource_id)?;
    insert_header(headers, "X-Api-App-Key", &config.app_key)?;
    insert_header(headers, "X-Api-Connect-Id", connect_id)?;
    Ok(request)
}

fn insert_header(headers: &mut HeaderMap, name: &'static str, value: &str) -> AiResult<()> {
    let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
        AiFoundationError::InvalidRequest(format!("invalid realtime header name {name}: {error}"))
    })?;
    let value = HeaderValue::from_str(value).map_err(|error| {
        AiFoundationError::InvalidRequest(format!("invalid realtime header {name}: {error}"))
    })?;
    headers.insert(name, value);
    Ok(())
}

fn client_event_to_frame(event: RealtimeClientEvent) -> AiResult<RealtimeFrame> {
    Ok(match event {
        RealtimeClientEvent::StartConnection { payload } => {
            json_frame(RealtimeEventId::StartConnection.as_u32(), None, payload)?
        }
        RealtimeClientEvent::FinishConnection => json_frame(
            RealtimeEventId::FinishConnection.as_u32(),
            None,
            serde_json::json!({}),
        )?,
        RealtimeClientEvent::StartSession {
            session_id,
            payload,
        } => json_frame(
            RealtimeEventId::StartSession.as_u32(),
            Some(session_id),
            payload,
        )?,
        RealtimeClientEvent::FinishSession { session_id } => json_frame(
            RealtimeEventId::FinishSession.as_u32(),
            Some(session_id),
            serde_json::json!({}),
        )?,
        RealtimeClientEvent::TaskRequest { session_id, audio } => {
            RealtimeFrame::audio_client(RealtimeEventId::TaskRequest, session_id, audio)
        }
        RealtimeClientEvent::UpdateConfig {
            session_id,
            payload,
        } => json_frame(
            RealtimeEventId::UpdateConfig.as_u32(),
            Some(session_id),
            payload,
        )?,
        RealtimeClientEvent::SayHello {
            session_id,
            content,
        } => json_frame(
            RealtimeEventId::SayHello.as_u32(),
            Some(session_id),
            serde_json::json!({ "content": content }),
        )?,
        RealtimeClientEvent::EndAsr { session_id } => json_frame(
            RealtimeEventId::EndAsr.as_u32(),
            Some(session_id),
            serde_json::json!({}),
        )?,
        RealtimeClientEvent::ChatTtsText {
            session_id,
            start,
            content,
            end,
        } => json_frame(
            RealtimeEventId::ChatTtsText.as_u32(),
            Some(session_id),
            serde_json::json!({ "start": start, "content": content, "end": end }),
        )?,
        RealtimeClientEvent::ChatTextQuery {
            session_id,
            content,
        } => json_frame(
            RealtimeEventId::ChatTextQuery.as_u32(),
            Some(session_id),
            serde_json::json!({ "content": content }),
        )?,
        RealtimeClientEvent::ChatRagText {
            session_id,
            external_rag,
        } => json_frame(
            RealtimeEventId::ChatRagText.as_u32(),
            Some(session_id),
            serde_json::json!({ "external_rag": external_rag }),
        )?,
        RealtimeClientEvent::ConversationCreate { session_id, items } => json_frame(
            RealtimeEventId::ConversationCreate.as_u32(),
            Some(session_id),
            serde_json::json!({ "items": items }),
        )?,
        RealtimeClientEvent::ConversationUpdate { session_id, items } => json_frame(
            RealtimeEventId::ConversationUpdate.as_u32(),
            Some(session_id),
            serde_json::json!({ "items": items }),
        )?,
        RealtimeClientEvent::ConversationRetrieve { session_id, items } => json_frame(
            RealtimeEventId::ConversationRetrieve.as_u32(),
            Some(session_id),
            serde_json::json!({ "items": items }),
        )?,
        RealtimeClientEvent::ConversationTruncate {
            session_id,
            item_id,
            audio_end_ms,
        } => json_frame(
            RealtimeEventId::ConversationTruncate.as_u32(),
            Some(session_id),
            serde_json::json!({ "item_id": item_id, "audio_end_ms": audio_end_ms }),
        )?,
        RealtimeClientEvent::ConversationDelete { session_id, items } => json_frame(
            RealtimeEventId::ConversationDelete.as_u32(),
            Some(session_id),
            serde_json::json!({ "items": items }),
        )?,
        RealtimeClientEvent::ClientInterrupt { session_id } => json_frame(
            RealtimeEventId::ClientInterrupt.as_u32(),
            Some(session_id),
            serde_json::json!({}),
        )?,
        RealtimeClientEvent::RawJson {
            event_id,
            session_id,
            payload,
        } => json_frame(event_id, session_id, payload)?,
        RealtimeClientEvent::RawAudio {
            event_id,
            session_id,
            audio,
        } => RealtimeFrame {
            message_type: MessageType::AudioOnlyRequest,
            serialization: Serialization::Raw,
            compression: crate::providers::ark::realtime_protocol::Compression::None,
            event_id: Some(event_id),
            code: None,
            sequence: None,
            connect_id: None,
            session_id,
            payload: audio,
        },
    })
}

fn json_frame(
    event_id: u32,
    session_id: Option<String>,
    payload: Value,
) -> AiResult<RealtimeFrame> {
    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        AiFoundationError::Serialization(format!(
            "failed to serialize realtime JSON payload: {error}"
        ))
    })?;
    Ok(RealtimeFrame {
        message_type: MessageType::FullClientRequest,
        serialization: Serialization::Json,
        compression: crate::providers::ark::realtime_protocol::Compression::None,
        event_id: Some(event_id),
        code: None,
        sequence: None,
        connect_id: None,
        session_id,
        payload: Bytes::from(bytes),
    })
}

fn frame_to_server_event(frame: RealtimeFrame) -> AiResult<RealtimeServerEvent> {
    if frame.message_type == MessageType::ErrorInformation {
        let raw = parse_json_payload(&frame).ok();
        let message = raw
            .as_ref()
            .and_then(|value| value.get("error").or_else(|| value.get("message")))
            .and_then(Value::as_str)
            .map(ToString::to_string);
        return Ok(RealtimeServerEvent::Error {
            code: frame.code,
            event_id: frame.event_id,
            session_id: frame.session_id,
            message,
            raw,
            payload: frame.payload,
        });
    }

    let event_id = frame.event_id.unwrap_or_default();
    if frame.message_type == MessageType::AudioOnlyResponse {
        return Ok(match RealtimeEventId::from_u32(event_id) {
            Some(RealtimeEventId::TtsResponse) => RealtimeServerEvent::TtsResponse {
                session_id: frame.session_id,
                audio: frame.payload,
            },
            _ => RealtimeServerEvent::RawAudio {
                event_id,
                session_id: frame.session_id,
                audio: frame.payload,
            },
        });
    }

    if frame.serialization != Serialization::Json {
        return Ok(RealtimeServerEvent::Raw {
            event_id: frame.event_id,
            session_id: frame.session_id,
            payload: frame.payload,
        });
    }

    let payload = parse_json_payload(&frame)?;
    Ok(match RealtimeEventId::from_u32(event_id) {
        Some(RealtimeEventId::ConnectionStarted) => {
            RealtimeServerEvent::ConnectionStarted { raw: payload }
        }
        Some(RealtimeEventId::ConnectionFailed) => RealtimeServerEvent::ConnectionFailed {
            error: string_at(&payload, "error"),
            raw: payload,
        },
        Some(RealtimeEventId::ConnectionFinished) => {
            RealtimeServerEvent::ConnectionFinished { raw: payload }
        }
        Some(RealtimeEventId::SessionStarted) => RealtimeServerEvent::SessionStarted {
            session_id: frame.session_id,
            dialog_id: string_at(&payload, "dialog_id"),
            raw: payload,
        },
        Some(RealtimeEventId::SessionFinished) => RealtimeServerEvent::SessionFinished {
            session_id: frame.session_id,
            raw: payload,
        },
        Some(RealtimeEventId::SessionFailed) => RealtimeServerEvent::SessionFailed {
            session_id: frame.session_id,
            error: string_at(&payload, "error"),
            raw: payload,
        },
        Some(RealtimeEventId::UsageResponse) => RealtimeServerEvent::UsageResponse {
            session_id: frame.session_id,
            usage: parse_usage(&payload),
            raw: payload,
        },
        Some(RealtimeEventId::ConfigUpdated) => RealtimeServerEvent::ConfigUpdated {
            session_id: frame.session_id,
            raw: payload,
        },
        Some(RealtimeEventId::TtsSentenceStart) => RealtimeServerEvent::TtsSentenceStart {
            session_id: frame.session_id,
            tts_type: string_at(&payload, "tts_type"),
            text: string_at(&payload, "text"),
            question_id: string_at(&payload, "question_id"),
            reply_id: string_at(&payload, "reply_id"),
            raw: payload,
        },
        Some(RealtimeEventId::TtsSentenceEnd) => RealtimeServerEvent::TtsSentenceEnd {
            session_id: frame.session_id,
            question_id: string_at(&payload, "question_id"),
            reply_id: string_at(&payload, "reply_id"),
            raw: payload,
        },
        Some(RealtimeEventId::TtsEnded) => RealtimeServerEvent::TtsEnded {
            session_id: frame.session_id,
            question_id: string_at(&payload, "question_id"),
            reply_id: string_at(&payload, "reply_id"),
            status_code: string_at(&payload, "status_code"),
            raw: payload,
        },
        Some(RealtimeEventId::AsrInfo) => RealtimeServerEvent::AsrInfo {
            session_id: frame.session_id,
            question_id: string_at(&payload, "question_id"),
            raw: payload,
        },
        Some(RealtimeEventId::AsrResponse) => RealtimeServerEvent::AsrResponse {
            session_id: frame.session_id,
            results: parse_asr_results(&payload),
            raw: payload,
        },
        Some(RealtimeEventId::AsrEnded) => RealtimeServerEvent::AsrEnded {
            session_id: frame.session_id,
            raw: payload,
        },
        Some(RealtimeEventId::ChatResponse) => RealtimeServerEvent::ChatResponse {
            session_id: frame.session_id,
            content: string_at(&payload, "content"),
            question_id: string_at(&payload, "question_id"),
            reply_id: string_at(&payload, "reply_id"),
            raw: payload,
        },
        Some(RealtimeEventId::ChatTextQueryConfirmed) => {
            RealtimeServerEvent::ChatTextQueryConfirmed {
                session_id: frame.session_id,
                question_id: string_at(&payload, "question_id"),
                raw: payload,
            }
        }
        Some(RealtimeEventId::ChatEnded) => RealtimeServerEvent::ChatEnded {
            session_id: frame.session_id,
            question_id: string_at(&payload, "question_id"),
            reply_id: string_at(&payload, "reply_id"),
            raw: payload,
        },
        Some(RealtimeEventId::ConversationCreated) => RealtimeServerEvent::ConversationCreated {
            session_id: frame.session_id,
            items: array_at(&payload, "items"),
            raw: payload,
        },
        Some(RealtimeEventId::ConversationUpdated) => RealtimeServerEvent::ConversationUpdated {
            session_id: frame.session_id,
            raw: payload,
        },
        Some(RealtimeEventId::ConversationRetrieved) => {
            RealtimeServerEvent::ConversationRetrieved {
                session_id: frame.session_id,
                items: array_at(&payload, "items"),
                raw: payload,
            }
        }
        Some(RealtimeEventId::ConversationTruncated) => {
            RealtimeServerEvent::ConversationTruncated {
                session_id: frame.session_id,
                raw: payload,
            }
        }
        Some(RealtimeEventId::ConversationDeleted) => RealtimeServerEvent::ConversationDeleted {
            session_id: frame.session_id,
            items: array_at(&payload, "items"),
            raw: payload,
        },
        Some(RealtimeEventId::DialogCommonError) => RealtimeServerEvent::DialogCommonError {
            session_id: frame.session_id,
            status_code: string_at(&payload, "status_code"),
            message: string_at(&payload, "message"),
            raw: payload,
        },
        _ => RealtimeServerEvent::RawJson {
            event_id,
            session_id: frame.session_id,
            payload,
        },
    })
}

fn parse_json_payload(frame: &RealtimeFrame) -> AiResult<Value> {
    if frame.payload.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_slice(&frame.payload).map_err(|error| {
        AiFoundationError::Serialization(format!(
            "failed to parse Ark realtime JSON payload: {error}"
        ))
    })
}

fn string_at(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn array_at(payload: &Value, key: &str) -> Vec<Value> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn parse_usage(payload: &Value) -> RealtimeUsage {
    let usage = payload.get("usage").unwrap_or(payload);
    RealtimeUsage {
        input_text_tokens: usage
            .get("input_text_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        input_audio_tokens: usage
            .get("input_audio_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        cached_text_tokens: usage
            .get("cached_text_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        cached_audio_tokens: usage
            .get("cached_audio_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        output_text_tokens: usage
            .get("output_text_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        output_audio_tokens: usage
            .get("output_audio_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    }
}

fn parse_asr_results(payload: &Value) -> Vec<RealtimeAsrResult> {
    payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(RealtimeAsrResult {
                text: item.get("text")?.as_str()?.to_string(),
                is_interim: item
                    .get("is_interim")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_chat_text_query_to_json_frame() {
        let frame = client_event_to_frame(RealtimeClientEvent::ChatTextQuery {
            session_id: "session".to_string(),
            content: "hello".to_string(),
        })
        .unwrap();

        assert_eq!(frame.message_type, MessageType::FullClientRequest);
        assert_eq!(
            frame.event_id,
            Some(RealtimeEventId::ChatTextQuery.as_u32())
        );
        assert_eq!(frame.session_id.as_deref(), Some("session"));
        assert_eq!(
            serde_json::from_slice::<Value>(&frame.payload).unwrap()["content"],
            "hello"
        );
    }

    #[test]
    fn maps_tts_response_frame_to_server_event() {
        let event = frame_to_server_event(RealtimeFrame {
            message_type: MessageType::AudioOnlyResponse,
            serialization: Serialization::Raw,
            compression: crate::providers::ark::realtime_protocol::Compression::None,
            event_id: Some(RealtimeEventId::TtsResponse.as_u32()),
            code: None,
            sequence: None,
            connect_id: None,
            session_id: Some("session".to_string()),
            payload: Bytes::from_static(&[1, 2, 3]),
        })
        .unwrap();

        match event {
            RealtimeServerEvent::TtsResponse { session_id, audio } => {
                assert_eq!(session_id.as_deref(), Some("session"));
                assert_eq!(audio, Bytes::from_static(&[1, 2, 3]));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn maps_asr_response_json() {
        let event = frame_to_server_event(RealtimeFrame {
            message_type: MessageType::FullServerResponse,
            serialization: Serialization::Json,
            compression: crate::providers::ark::realtime_protocol::Compression::None,
            event_id: Some(RealtimeEventId::AsrResponse.as_u32()),
            code: None,
            sequence: None,
            connect_id: None,
            session_id: Some("session".to_string()),
            payload: Bytes::from_static(br#"{"results":[{"text":"hi","is_interim":true}]}"#),
        })
        .unwrap();

        match event {
            RealtimeServerEvent::AsrResponse { results, .. } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].text, "hi");
                assert!(results[0].is_interim);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
