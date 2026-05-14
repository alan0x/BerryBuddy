use serde::{Deserialize, Serialize};

use crate::operation::{Operation, Provider};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub provider: Provider,
    pub model_id: String,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: Option<Provider>,
    pub model_id: String,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextGenerationResponse {
    pub text: String,
    pub usage: Usage,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
    pub image_data: String,
    pub mime_type: String,
    pub usage: Usage,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenerationResponse {
    pub video_data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_frame_url: Option<String>,
    pub model: String,
    pub operation_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioGenerationResponse {
    pub audio_data: String,
    pub mime_type: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechGenerationResponse {
    pub audio_data: String,
    pub mime_type: String,
    pub sample_rate: u32,
    pub usage: Usage,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiAssetRef {
    Url { url: String },
    LocalPath { path: String },
    StoragePath { path: String },
    Base64 { mime_type: String, data: String },
    DataUrl { data_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedAsset {
    pub data: AiAssetRef,
    pub mime_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_secs: Option<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderOutput<T> {
    pub data: T,
    pub usage: Option<Usage>,
    pub provider: Provider,
    pub model_id: String,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum ToolCallingResponse {
    Text {
        text: String,
        usage: Option<Usage>,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
        usage: Option<Usage>,
        raw_part: Option<serde_json::Value>,
    },
}
