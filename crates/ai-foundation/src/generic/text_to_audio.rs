use crate::error::AiResult;
use crate::generic::types::TextToAudioRequest;
use crate::providers::router::ProviderRouter;
use crate::types::AudioGenerationResponse;

pub async fn run(
    router: &ProviderRouter,
    request: TextToAudioRequest,
) -> AiResult<AudioGenerationResponse> {
    let payload = serde_json::json!({
        "instances": [{ "prompt": request.prompt }],
        "parameters": {
            "seed": request.seed,
            "sampleCount": request.sample_count,
            "voice": request.voice,
            "response_format": request.response_format,
        }
    });
    router.text_to_audio(&request.model.model_id, payload).await
}

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: serde_json::Value,
) -> AiResult<AudioGenerationResponse> {
    router.text_to_audio(model_id, payload).await
}
