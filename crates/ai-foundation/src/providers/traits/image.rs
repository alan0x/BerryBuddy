use async_trait::async_trait;
use serde_json::Value;

use crate::error::AiResult;
use crate::types::{AiProviderOutput, ImageGenerationResponse};

#[derive(Debug, Clone)]
pub struct ProviderImageRequest {
    pub model_id: String,
    pub payload: Value,
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn text_to_image(
        &self,
        request: ProviderImageRequest,
    ) -> AiResult<AiProviderOutput<ImageGenerationResponse>>;
}
