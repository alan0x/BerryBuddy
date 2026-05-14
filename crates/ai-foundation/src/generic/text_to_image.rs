use crate::error::AiResult;
use crate::generic::types::TextToImageRequest;
use crate::providers::router::ProviderRouter;
use crate::types::ImageGenerationResponse;

pub async fn run(
    router: &ProviderRouter,
    request: TextToImageRequest,
) -> AiResult<ImageGenerationResponse> {
    let payload = serde_json::json!({
        "instances": [{ "prompt": request.prompt }],
        "parameters": {
            "aspectRatio": request.aspect_ratio.unwrap_or_else(|| "1:1".to_string()),
            "sampleCount": request.sample_count.unwrap_or(1),
            "negativePrompt": request.negative_prompt,
            "personGeneration": request.person_generation,
            "safetySetting": request.safety_setting,
            "addWatermark": request.add_watermark,
            "includeRaiReason": request.include_rai_reason,
            "language": request.language,
        }
    });
    router.text_to_image(&request.model.model_id, payload).await
}

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: serde_json::Value,
) -> AiResult<ImageGenerationResponse> {
    router.text_to_image(model_id, payload).await
}
