use async_trait::async_trait;
use serde_json::Value;

use crate::error::AiResult;
use crate::types::{AiProviderOutput, TextGenerationResponse};

#[derive(Debug, Clone)]
pub struct ProviderTextRequest {
    pub model_id: String,
    pub payload: Value,
}

#[async_trait]
pub trait TextProvider: Send + Sync {
    async fn text_to_text(
        &self,
        request: ProviderTextRequest,
    ) -> AiResult<AiProviderOutput<TextGenerationResponse>>;
}
