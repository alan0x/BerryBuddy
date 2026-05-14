use serde_json::{Map, Value};

use crate::error::AiResult;
use crate::generic::types::TextToTextRequest;
use crate::providers::router::ProviderRouter;
use crate::types::TextGenerationResponse;

pub async fn run(
    router: &ProviderRouter,
    request: TextToTextRequest,
) -> AiResult<TextGenerationResponse> {
    let payload = build_legacy_payload(&request);
    router.text_to_text(&request.model.model_id, payload).await
}

pub async fn run_payload(
    router: &ProviderRouter,
    model_id: &str,
    payload: Value,
) -> AiResult<TextGenerationResponse> {
    router.text_to_text(model_id, payload).await
}

pub fn build_legacy_payload(request: &TextToTextRequest) -> Value {
    let mut root = Map::new();

    if let Some(system_prompt) = &request.system_prompt {
        root.insert(
            "system_instruction".to_string(),
            serde_json::json!({ "parts": [{ "text": system_prompt }] }),
        );
    }

    root.insert(
        "contents".to_string(),
        serde_json::json!([{ "role": "user", "parts": [{ "text": request.user_prompt }] }]),
    );

    add_generation_options(&mut root, &request.options);
    Value::Object(root)
}

pub(crate) fn add_generation_options(
    root: &mut Map<String, Value>,
    options: &crate::generic::types::GenerationOptions,
) {
    let mut generation_config = Map::new();
    if let Some(temperature) = options.temperature {
        generation_config.insert("temperature".to_string(), serde_json::json!(temperature));
    }
    if let Some(max_output_tokens) = options.max_output_tokens {
        generation_config.insert(
            "maxOutputTokens".to_string(),
            serde_json::json!(max_output_tokens),
        );
    }
    if let Some(response_mime_type) = &options.response_mime_type {
        generation_config.insert(
            "responseMimeType".to_string(),
            serde_json::json!(response_mime_type),
        );
    }
    if let Some(response_schema) = &options.response_schema {
        generation_config.insert("responseSchema".to_string(), response_schema.clone());
    }
    if let Some(response_modalities) = &options.response_modalities {
        generation_config.insert(
            "responseModalities".to_string(),
            serde_json::json!(response_modalities),
        );
    }
    if !generation_config.is_empty() {
        root.insert(
            "generationConfig".to_string(),
            Value::Object(generation_config),
        );
    }
    if let Some(safety_settings) = &options.safety_settings {
        root.insert(
            "safetySettings".to_string(),
            Value::Array(safety_settings.clone()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_legacy_vertex_style_payload() {
        let mut request = TextToTextRequest::new("gemini-2.5-pro", "write a script");
        request.system_prompt = Some("You are a TVC planner".to_string());
        request.options.temperature = Some(0.4);
        request.options.response_mime_type = Some("application/json".to_string());

        let payload = build_legacy_payload(&request);

        assert_eq!(
            payload
                .pointer("/system_instruction/parts/0/text")
                .and_then(Value::as_str),
            Some("You are a TVC planner")
        );
        assert_eq!(
            payload
                .pointer("/contents/0/parts/0/text")
                .and_then(Value::as_str),
            Some("write a script")
        );
        let temperature = payload
            .pointer("/generationConfig/temperature")
            .and_then(Value::as_f64)
            .unwrap();
        assert!((temperature - 0.4).abs() < 0.000001);
        assert_eq!(
            payload
                .pointer("/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            Some("application/json")
        );
    }
}
