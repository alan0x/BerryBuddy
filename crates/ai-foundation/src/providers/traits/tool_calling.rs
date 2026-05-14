use async_trait::async_trait;
use serde_json::Value;

use crate::error::AiResult;
use crate::types::{AiProviderOutput, ToolCallingResponse};

#[derive(Debug, Clone)]
pub struct ProviderToolCallingRequest {
    pub model_id: String,
    pub payload: Value,
}

#[async_trait]
pub trait ToolCallingProvider: Send + Sync {
    async fn tool_calling(
        &self,
        request: ProviderToolCallingRequest,
    ) -> AiResult<AiProviderOutput<ToolCallingResponse>>;
}
