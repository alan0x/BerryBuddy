use crate::error::AiResult;
use crate::providers::router::ProviderRouter;
use crate::types::TextGenerationResponse;

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: serde_json::Value,
) -> AiResult<TextGenerationResponse> {
    router.video_to_text(model_id, payload).await
}
