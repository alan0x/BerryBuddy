use crate::error::AiResult;
use crate::generic::types::TextToSpeechRequest;
use crate::providers::router::ProviderRouter;
use crate::types::SpeechGenerationResponse;

pub async fn run(
    router: &ProviderRouter,
    request: TextToSpeechRequest,
) -> AiResult<SpeechGenerationResponse> {
    let payload = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": request.text }] }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "languageCode": request.language_code,
                "voiceConfig": request.voice_name.map(|voice| serde_json::json!({ "prebuiltVoiceConfig": { "voiceName": voice } })),
            }
        }
    });
    router
        .text_to_speech(&request.model.model_id, payload)
        .await
}

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: serde_json::Value,
) -> AiResult<SpeechGenerationResponse> {
    router.text_to_speech(model_id, payload).await
}
