use async_trait::async_trait;
use serde_json::Value;

use crate::error::AiResult;
use crate::types::{AiProviderOutput, AudioGenerationResponse, SpeechGenerationResponse};

#[derive(Debug, Clone)]
pub struct ProviderAudioRequest {
    pub model_id: String,
    pub payload: Value,
}

#[async_trait]
pub trait AudioProvider: Send + Sync {
    async fn text_to_audio(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<AudioGenerationResponse>>;
    async fn text_to_speech(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<SpeechGenerationResponse>>;
}
