use crate::error::AiResult;
use crate::providers::router::ProviderRouter;
use crate::types::ImageGenerationResponse;

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: serde_json::Value,
) -> AiResult<ImageGenerationResponse> {
    router.image_to_image(model_id, payload).await
}
