use serde::de::DeserializeOwned;

use crate::error::{AiFoundationError, AiResult};
use crate::generic::text_to_text;
use crate::generic::types::TextToTextRequest;
use crate::providers::router::ProviderRouter;
use crate::types::TextGenerationResponse;

pub struct StructuredOutputResponse {
    pub value: serde_json::Value,
    pub text_response: TextGenerationResponse,
}

pub async fn run_json(
    router: &ProviderRouter,
    request: TextToTextRequest,
) -> AiResult<serde_json::Value> {
    Ok(run_json_with_response(router, request).await?.value)
}

pub async fn run_json_with_response(
    router: &ProviderRouter,
    request: TextToTextRequest,
) -> AiResult<StructuredOutputResponse> {
    let text_response = text_to_text::run(router, request).await?;
    let value = parse_json_text(&text_response.text)?;
    Ok(StructuredOutputResponse {
        value,
        text_response,
    })
}

pub async fn run<T>(router: &ProviderRouter, request: TextToTextRequest) -> AiResult<T>
where
    T: DeserializeOwned,
{
    let value = run_json(router, request).await?;
    serde_json::from_value(value)
        .map_err(|error| AiFoundationError::Serialization(error.to_string()))
}

fn parse_json_text(text: &str) -> AiResult<serde_json::Value> {
    let text = strip_markdown_fences(text.trim());
    serde_json::from_str(text).map_err(|error| AiFoundationError::Serialization(error.to_string()))
}

fn strip_markdown_fences(text: &str) -> &str {
    if text.starts_with("```") {
        let inner = text.trim_start_matches("```json").trim_start_matches("```");
        inner.trim_end_matches("```").trim()
    } else {
        text
    }
}
