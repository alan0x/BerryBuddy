use serde_json::{Map, Value};

use crate::error::AiResult;
use crate::generic::text_to_text::add_generation_options;
use crate::generic::types::TextChatRequest;
use crate::providers::router::ProviderRouter;
use crate::types::TextGenerationResponse;

pub async fn run(
    router: &ProviderRouter,
    request: TextChatRequest,
) -> AiResult<TextGenerationResponse> {
    router
        .text_to_text(&request.model.model_id, build_payload(&request))
        .await
}

fn build_payload(request: &TextChatRequest) -> Value {
    let mut root = Map::new();
    if let Some(system_prompt) = &request.system_prompt {
        root.insert(
            "system_instruction".to_string(),
            serde_json::json!({ "parts": [{ "text": system_prompt }] }),
        );
    }
    root.insert(
        "contents".to_string(),
        Value::Array(request.contents.clone()),
    );
    add_generation_options(&mut root, &request.options);
    Value::Object(root)
}
