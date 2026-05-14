use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AiResult;
use crate::operation::Provider;

#[derive(Debug, Clone)]
pub struct RealtimeConnectRequest {
    pub connect_id: Option<String>,
    pub auto_start_connection: bool,
    pub start_connection_payload: Value,
}

impl Default for RealtimeConnectRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl RealtimeConnectRequest {
    pub fn new() -> Self {
        Self {
            connect_id: None,
            auto_start_connection: true,
            start_connection_payload: serde_json::json!({}),
        }
    }

    pub fn with_connect_id(mut self, connect_id: impl Into<String>) -> Self {
        self.connect_id = Some(connect_id.into());
        self
    }

    pub fn without_start_connection(mut self) -> Self {
        self.auto_start_connection = false;
        self
    }

    pub fn with_start_connection_payload(mut self, payload: Value) -> Self {
        self.start_connection_payload = payload;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeStartSessionRequest {
    pub session_id: String,
    pub payload: Value,
}

impl RealtimeStartSessionRequest {
    pub fn new(session_id: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        Self {
            session_id: session_id.into(),
            payload: serde_json::json!({
                "tts": {
                    "extra": {},
                    "audio_config": {
                        "channel": 1,
                        "format": "pcm_s16le",
                        "sample_rate": 24000
                    }
                },
                "asr": {
                    "extra": {},
                    "audio_info": {
                        "format": "pcm_s16le",
                        "sample_rate": 16000,
                        "channel": 1
                    }
                },
                "dialog": {
                    "dialog_id": "",
                    "extra": {
                        "input_mod": "push_to_talk",
                        "model": model
                    }
                }
            }),
        }
    }

    pub fn from_payload(session_id: impl Into<String>, payload: Value) -> Self {
        Self {
            session_id: session_id.into(),
            payload,
        }
    }

    pub fn with_bot_name(mut self, bot_name: impl Into<String>) -> Self {
        self.payload["dialog"]["bot_name"] = Value::String(bot_name.into());
        self
    }

    pub fn with_system_role(mut self, system_role: impl Into<String>) -> Self {
        self.payload["dialog"]["system_role"] = Value::String(system_role.into());
        self
    }

    pub fn with_speaking_style(mut self, speaking_style: impl Into<String>) -> Self {
        self.payload["dialog"]["speaking_style"] = Value::String(speaking_style.into());
        self
    }

    pub fn with_character_manifest(mut self, character_manifest: impl Into<String>) -> Self {
        self.payload["dialog"]["character_manifest"] = Value::String(character_manifest.into());
        self
    }

    pub fn with_speaker(mut self, speaker: impl Into<String>) -> Self {
        self.payload["tts"]["speaker"] = Value::String(speaker.into());
        self
    }

    pub fn with_input_mode(mut self, input_mode: RealtimeInputMode) -> Self {
        self.payload["dialog"]["extra"]["input_mod"] =
            Value::String(input_mode.as_str().to_string());
        self
    }

    pub fn with_asr_audio(
        mut self,
        format: impl Into<String>,
        sample_rate: u32,
        channel: u32,
    ) -> Self {
        self.payload["asr"]["audio_info"] = serde_json::json!({
            "format": format.into(),
            "sample_rate": sample_rate,
            "channel": channel,
        });
        self
    }

    pub fn with_tts_audio(
        mut self,
        format: impl Into<String>,
        sample_rate: u32,
        channel: u32,
    ) -> Self {
        self.payload["tts"]["audio_config"] = serde_json::json!({
            "format": format.into(),
            "sample_rate": sample_rate,
            "channel": channel,
        });
        self
    }

    pub fn merge_payload(mut self, patch: Value) -> Self {
        merge_json(&mut self.payload, patch);
        self
    }
}

fn merge_json(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge_json(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeInputMode {
    Microphone,
    KeepAlive,
    PushToTalk,
    Text,
    AudioFile,
}

impl RealtimeInputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::KeepAlive => "keep_alive",
            Self::PushToTalk => "push_to_talk",
            Self::Text => "text",
            Self::AudioFile => "audio_file",
        }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RealtimeEventId {
    StartConnection = 1,
    FinishConnection = 2,
    ConnectionStarted = 50,
    ConnectionFailed = 51,
    ConnectionFinished = 52,
    StartSession = 100,
    FinishSession = 102,
    SessionStarted = 150,
    SessionFinished = 152,
    SessionFailed = 153,
    UsageResponse = 154,
    TaskRequest = 200,
    UpdateConfig = 201,
    ConfigUpdated = 251,
    SayHello = 300,
    TtsSentenceStart = 350,
    TtsSentenceEnd = 351,
    TtsResponse = 352,
    TtsEnded = 359,
    EndAsr = 400,
    AsrInfo = 450,
    AsrResponse = 451,
    AsrEnded = 459,
    ChatTtsText = 500,
    ChatTextQuery = 501,
    ChatRagText = 502,
    ConversationCreate = 510,
    ConversationUpdate = 511,
    ConversationRetrieve = 512,
    ConversationTruncate = 513,
    ConversationDelete = 514,
    ClientInterrupt = 515,
    ChatResponse = 550,
    ChatTextQueryConfirmed = 553,
    ChatEnded = 559,
    ConversationCreated = 567,
    ConversationUpdated = 568,
    ConversationRetrieved = 569,
    ConversationTruncated = 570,
    ConversationDeleted = 571,
    DialogCommonError = 599,
}

impl RealtimeEventId {
    pub fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::StartConnection,
            2 => Self::FinishConnection,
            50 => Self::ConnectionStarted,
            51 => Self::ConnectionFailed,
            52 => Self::ConnectionFinished,
            100 => Self::StartSession,
            102 => Self::FinishSession,
            150 => Self::SessionStarted,
            152 => Self::SessionFinished,
            153 => Self::SessionFailed,
            154 => Self::UsageResponse,
            200 => Self::TaskRequest,
            201 => Self::UpdateConfig,
            251 => Self::ConfigUpdated,
            300 => Self::SayHello,
            350 => Self::TtsSentenceStart,
            351 => Self::TtsSentenceEnd,
            352 => Self::TtsResponse,
            359 => Self::TtsEnded,
            400 => Self::EndAsr,
            450 => Self::AsrInfo,
            451 => Self::AsrResponse,
            459 => Self::AsrEnded,
            500 => Self::ChatTtsText,
            501 => Self::ChatTextQuery,
            502 => Self::ChatRagText,
            510 => Self::ConversationCreate,
            511 => Self::ConversationUpdate,
            512 => Self::ConversationRetrieve,
            513 => Self::ConversationTruncate,
            514 => Self::ConversationDelete,
            515 => Self::ClientInterrupt,
            550 => Self::ChatResponse,
            553 => Self::ChatTextQueryConfirmed,
            559 => Self::ChatEnded,
            567 => Self::ConversationCreated,
            568 => Self::ConversationUpdated,
            569 => Self::ConversationRetrieved,
            570 => Self::ConversationTruncated,
            571 => Self::ConversationDeleted,
            599 => Self::DialogCommonError,
            _ => return None,
        })
    }

    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn has_session_id(self) -> bool {
        !matches!(
            self,
            Self::StartConnection
                | Self::FinishConnection
                | Self::ConnectionStarted
                | Self::ConnectionFailed
                | Self::ConnectionFinished
        )
    }
}

#[derive(Debug, Clone)]
pub enum RealtimeClientEvent {
    StartConnection {
        payload: Value,
    },
    FinishConnection,
    StartSession {
        session_id: String,
        payload: Value,
    },
    FinishSession {
        session_id: String,
    },
    TaskRequest {
        session_id: String,
        audio: Bytes,
    },
    UpdateConfig {
        session_id: String,
        payload: Value,
    },
    SayHello {
        session_id: String,
        content: String,
    },
    EndAsr {
        session_id: String,
    },
    ChatTtsText {
        session_id: String,
        start: bool,
        content: String,
        end: bool,
    },
    ChatTextQuery {
        session_id: String,
        content: String,
    },
    ChatRagText {
        session_id: String,
        external_rag: String,
    },
    ConversationCreate {
        session_id: String,
        items: Vec<Value>,
    },
    ConversationUpdate {
        session_id: String,
        items: Vec<Value>,
    },
    ConversationRetrieve {
        session_id: String,
        items: Vec<Value>,
    },
    ConversationTruncate {
        session_id: String,
        item_id: String,
        audio_end_ms: i64,
    },
    ConversationDelete {
        session_id: String,
        items: Vec<Value>,
    },
    ClientInterrupt {
        session_id: String,
    },
    RawJson {
        event_id: u32,
        session_id: Option<String>,
        payload: Value,
    },
    RawAudio {
        event_id: u32,
        session_id: Option<String>,
        audio: Bytes,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeAsrResult {
    pub text: String,
    pub is_interim: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealtimeUsage {
    pub input_text_tokens: i64,
    pub input_audio_tokens: i64,
    pub cached_text_tokens: i64,
    pub cached_audio_tokens: i64,
    pub output_text_tokens: i64,
    pub output_audio_tokens: i64,
}

#[derive(Debug, Clone)]
pub enum RealtimeServerEvent {
    ConnectionStarted {
        raw: Value,
    },
    ConnectionFailed {
        error: Option<String>,
        raw: Value,
    },
    ConnectionFinished {
        raw: Value,
    },
    SessionStarted {
        session_id: Option<String>,
        dialog_id: Option<String>,
        raw: Value,
    },
    SessionFinished {
        session_id: Option<String>,
        raw: Value,
    },
    SessionFailed {
        session_id: Option<String>,
        error: Option<String>,
        raw: Value,
    },
    UsageResponse {
        session_id: Option<String>,
        usage: RealtimeUsage,
        raw: Value,
    },
    ConfigUpdated {
        session_id: Option<String>,
        raw: Value,
    },
    TtsSentenceStart {
        session_id: Option<String>,
        tts_type: Option<String>,
        text: Option<String>,
        question_id: Option<String>,
        reply_id: Option<String>,
        raw: Value,
    },
    TtsSentenceEnd {
        session_id: Option<String>,
        question_id: Option<String>,
        reply_id: Option<String>,
        raw: Value,
    },
    TtsResponse {
        session_id: Option<String>,
        audio: Bytes,
    },
    TtsEnded {
        session_id: Option<String>,
        question_id: Option<String>,
        reply_id: Option<String>,
        status_code: Option<String>,
        raw: Value,
    },
    AsrInfo {
        session_id: Option<String>,
        question_id: Option<String>,
        raw: Value,
    },
    AsrResponse {
        session_id: Option<String>,
        results: Vec<RealtimeAsrResult>,
        raw: Value,
    },
    AsrEnded {
        session_id: Option<String>,
        raw: Value,
    },
    ChatResponse {
        session_id: Option<String>,
        content: Option<String>,
        question_id: Option<String>,
        reply_id: Option<String>,
        raw: Value,
    },
    ChatTextQueryConfirmed {
        session_id: Option<String>,
        question_id: Option<String>,
        raw: Value,
    },
    ChatEnded {
        session_id: Option<String>,
        question_id: Option<String>,
        reply_id: Option<String>,
        raw: Value,
    },
    ConversationCreated {
        session_id: Option<String>,
        items: Vec<Value>,
        raw: Value,
    },
    ConversationUpdated {
        session_id: Option<String>,
        raw: Value,
    },
    ConversationRetrieved {
        session_id: Option<String>,
        items: Vec<Value>,
        raw: Value,
    },
    ConversationTruncated {
        session_id: Option<String>,
        raw: Value,
    },
    ConversationDeleted {
        session_id: Option<String>,
        items: Vec<Value>,
        raw: Value,
    },
    DialogCommonError {
        session_id: Option<String>,
        status_code: Option<String>,
        message: Option<String>,
        raw: Value,
    },
    Error {
        code: Option<u32>,
        event_id: Option<u32>,
        session_id: Option<String>,
        message: Option<String>,
        raw: Option<Value>,
        payload: Bytes,
    },
    RawJson {
        event_id: u32,
        session_id: Option<String>,
        payload: Value,
    },
    RawAudio {
        event_id: u32,
        session_id: Option<String>,
        audio: Bytes,
    },
    Raw {
        event_id: Option<u32>,
        session_id: Option<String>,
        payload: Bytes,
    },
}

#[async_trait]
pub trait RealtimeConversationSession: Send + Sync {
    fn provider(&self) -> Provider;
    fn connect_id(&self) -> &str;

    async fn send_client_event(&self, event: RealtimeClientEvent) -> AiResult<()>;
    async fn next_event(&self) -> AiResult<Option<RealtimeServerEvent>>;
    async fn close(&self) -> AiResult<()>;

    async fn start_connection(&self) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::StartConnection {
            payload: serde_json::json!({}),
        })
        .await
    }

    async fn start_session(&self, request: RealtimeStartSessionRequest) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::StartSession {
            session_id: request.session_id,
            payload: request.payload,
        })
        .await
    }

    async fn finish_session(&self, session_id: String) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::FinishSession { session_id })
            .await
    }

    async fn send_audio(&self, session_id: String, audio: Bytes) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::TaskRequest { session_id, audio })
            .await
    }

    async fn end_asr(&self, session_id: String) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::EndAsr { session_id })
            .await
    }

    async fn interrupt(&self, session_id: String) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::ClientInterrupt { session_id })
            .await
    }

    async fn send_text_query(&self, session_id: String, content: String) -> AiResult<()> {
        self.send_client_event(RealtimeClientEvent::ChatTextQuery {
            session_id,
            content,
        })
        .await
    }
}

#[async_trait]
pub trait RealtimeConversationProvider: Send + Sync {
    async fn connect_realtime_conversation(
        &self,
        request: RealtimeConnectRequest,
    ) -> AiResult<Arc<dyn RealtimeConversationSession>>;
}
