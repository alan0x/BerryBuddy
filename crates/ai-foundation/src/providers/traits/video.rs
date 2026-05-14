use async_trait::async_trait;

use crate::error::AiResult;
use crate::generic::types::VideoGenerationRequest;
use crate::types::VideoGenerationResponse;

#[derive(Debug, Clone)]
pub struct ProviderVideoStartRequest {
    pub request: VideoGenerationRequest,
}

#[derive(Debug, Clone)]
pub struct ProviderVideoPollRequest {
    pub model_id: String,
    pub operation_name: String,
    pub poll_interval_secs: Option<u64>,
    pub max_wait_secs: Option<u64>,
}

#[async_trait]
pub trait VideoProvider: Send + Sync {
    async fn start_video_generation(&self, request: ProviderVideoStartRequest) -> AiResult<String>;
    async fn poll_video_generation(
        &self,
        request: ProviderVideoPollRequest,
    ) -> AiResult<VideoGenerationResponse>;
}
