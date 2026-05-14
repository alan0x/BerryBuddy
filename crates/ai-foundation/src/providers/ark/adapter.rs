use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::error::{AiFoundationError, AiResult};
use crate::generic::types::{VideoGenerationMode, VideoGenerationRequest, VideoReferenceKind};
use crate::media::MediaInput;
use crate::operation::Provider;
use crate::providers::ark::config::ArkProviderConfig;
use crate::providers::traits::audio::{AudioProvider, ProviderAudioRequest};
use crate::providers::traits::image::{ImageProvider, ProviderImageRequest};
use crate::providers::traits::text::{ProviderTextRequest, TextProvider};
use crate::providers::traits::tool_calling::{ProviderToolCallingRequest, ToolCallingProvider};
use crate::providers::traits::video::{
    ProviderVideoPollRequest, ProviderVideoStartRequest, VideoProvider,
};
use crate::types::{
    AiProviderOutput, AudioGenerationResponse, ImageGenerationResponse, SpeechGenerationResponse,
    TextGenerationResponse, ToolCallingResponse, Usage, VideoGenerationResponse,
};

#[derive(Debug, Clone)]
pub struct ArkProvider {
    config: ArkProviderConfig,
}

impl ArkProvider {
    pub fn new(config: ArkProviderConfig) -> Self {
        Self { config }
    }

    fn client(&self) -> AiResult<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .build()
            .map_err(|error| AiFoundationError::Network(error.to_string()))
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn post_json(&self, path: &str, body: &Value) -> AiResult<Value> {
        let response = self
            .client()?
            .post(self.url(path))
            .header(AUTHORIZATION, format!("Bearer {}", self.config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(format!(
                "Ark request failed: path={} status={} body={}",
                path, status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|error| AiFoundationError::Serialization(error.to_string()))
    }
}

#[async_trait]
impl TextProvider for ArkProvider {
    async fn text_to_text(
        &self,
        request: ProviderTextRequest,
    ) -> AiResult<AiProviderOutput<TextGenerationResponse>> {
        let body = convert_vertex_payload_to_ark_body(&request.model_id, request.payload);
        let response_json = self.post_json("chat/completions", &body).await?;
        let text = extract_ark_message_text(&response_json).ok_or_else(|| {
            provider_error("Ark response missing choices[0].message.content text".to_string())
        })?;
        let usage = Usage {
            prompt_tokens: response_json
                .pointer("/usage/prompt_tokens")
                .and_then(|value| value.as_i64())
                .unwrap_or(0),
            completion_tokens: response_json
                .pointer("/usage/completion_tokens")
                .and_then(|value| value.as_i64())
                .unwrap_or(0),
            total_tokens: response_json
                .pointer("/usage/total_tokens")
                .and_then(|value| value.as_i64())
                .unwrap_or(0),
            cached_tokens: None,
        };
        let response = TextGenerationResponse {
            text,
            usage: usage.clone(),
            model: request.model_id.clone(),
            raw_response: Some(response_json.clone()),
        };
        Ok(output(
            request.model_id,
            Some(usage),
            Some(response_json),
            response,
        ))
    }
}

#[async_trait]
impl ImageProvider for ArkProvider {
    async fn text_to_image(
        &self,
        request: ProviderImageRequest,
    ) -> AiResult<AiProviderOutput<ImageGenerationResponse>> {
        let prompt = request
            .payload
            .get("prompt")
            .and_then(|value| value.as_str())
            .or_else(|| {
                request
                    .payload
                    .pointer("/instances/0/prompt")
                    .and_then(|value| value.as_str())
            })
            .map(ToString::to_string)
            .or_else(|| extract_text_prompt_from_contents(&request.payload))
            .ok_or_else(|| {
                provider_error(
                    "Ark generate_image requires payload.prompt or instances[0].prompt".to_string(),
                )
            })?;
        let aspect_ratio = request
            .payload
            .pointer("/parameters/aspectRatio")
            .and_then(|value| value.as_str())
            .or_else(|| {
                request
                    .payload
                    .pointer("/generationConfig/imageConfig/aspectRatio")
                    .and_then(Value::as_str)
            })
            .unwrap_or("1:1");
        let size = match aspect_ratio {
            "9:16" => "1024x1792",
            "16:9" => "1792x1024",
            _ => "1024x1024",
        };
        let body = serde_json::json!({
            "model": request.model_id,
            "prompt": prompt,
            "response_format": "b64_json",
            "size": size,
        });
        let response_json = self.post_json("images/generations", &body).await?;
        let image_data = response_json
            .pointer("/data/0/b64_json")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                provider_error("Ark image response missing data[0].b64_json".to_string())
            })?
            .to_string();
        let response = ImageGenerationResponse {
            image_data,
            mime_type: "image/png".to_string(),
            usage: Usage::default(),
            model: body["model"].as_str().unwrap_or_default().to_string(),
        };
        Ok(output(
            response.model.clone(),
            Some(Usage::default()),
            Some(response_json),
            response,
        ))
    }
}

#[async_trait]
impl AudioProvider for ArkProvider {
    async fn text_to_audio(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<AudioGenerationResponse>> {
        let body = if request.payload.get("input").is_some() {
            let mut body = request.payload;
            if body.get("model").is_none()
                && let Some(obj) = body.as_object_mut()
            {
                obj.insert("model".to_string(), Value::String(request.model_id.clone()));
            }
            body
        } else {
            let input = request
                .payload
                .get("prompt")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    request
                        .payload
                        .pointer("/instances/0/prompt")
                        .and_then(|value| value.as_str())
                })
                .ok_or_else(|| {
                    provider_error(
                        "Ark generate_audio requires input/prompt/instances[0].prompt".to_string(),
                    )
                })?;
            serde_json::json!({
                "model": request.model_id,
                "input": input,
                "voice": request.payload.get("voice").and_then(|value| value.as_str()).unwrap_or("alloy"),
                "response_format": request.payload
                    .get("response_format")
                    .or_else(|| request.payload.get("format"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("wav"),
            })
        };
        let response = self
            .client()?
            .post(self.url("audio/speech"))
            .header(AUTHORIZATION, format!("Bearer {}", self.config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(format!(
                "Ark audio generation failed: status={} body={}",
                status, body
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("audio/wav")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;
        let data = AudioGenerationResponse {
            audio_data: BASE64.encode(bytes),
            mime_type: content_type,
            model: body
                .get("model")
                .and_then(|value| value.as_str())
                .unwrap_or(&request.model_id)
                .to_string(),
        };
        Ok(output(data.model.clone(), None, None, data))
    }

    async fn text_to_speech(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<SpeechGenerationResponse>> {
        let input = extract_speech_input(&request.payload).ok_or_else(|| {
            provider_error(
                "Ark text_to_speech requires text in Gemini contents or input/prompt".to_string(),
            )
        })?;
        let body = serde_json::json!({
            "model": request.model_id,
            "input": input,
            "voice": normalize_ark_voice(extract_speech_voice(&request.payload)),
            "response_format": request
                .payload
                .get("response_format")
                .or_else(|| request.payload.get("format"))
                .and_then(Value::as_str)
                .unwrap_or("wav"),
        });

        let response = self
            .client()?
            .post(self.url("audio/speech"))
            .header(AUTHORIZATION, format!("Bearer {}", self.config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(format!(
                "Ark speech generation failed: status={} body={}",
                status, body
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("audio/wav")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;
        let data = SpeechGenerationResponse {
            audio_data: BASE64.encode(bytes),
            mime_type: content_type,
            sample_rate: 24000,
            usage: Usage::default(),
            model: body
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(&request.model_id)
                .to_string(),
        };
        Ok(output(
            data.model.clone(),
            Some(Usage::default()),
            None,
            data,
        ))
    }
}

#[async_trait]
impl VideoProvider for ArkProvider {
    async fn start_video_generation(&self, request: ProviderVideoStartRequest) -> AiResult<String> {
        let body = convert_video_request_to_ark_body(&request.request)?;
        let response_json = self.post_json("contents/generations/tasks", &body).await?;
        response_json
            .get("id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .ok_or_else(|| provider_error("Ark video generation response missing id".to_string()))
    }

    async fn poll_video_generation(
        &self,
        request: ProviderVideoPollRequest,
    ) -> AiResult<VideoGenerationResponse> {
        let poll_interval = Duration::from_secs(request.poll_interval_secs.unwrap_or(10));
        let max_wait = Duration::from_secs(request.max_wait_secs.unwrap_or(600));
        let started = Instant::now();
        let path = format!("contents/generations/tasks/{}", request.operation_name);

        loop {
            if started.elapsed() > max_wait {
                return Err(provider_error(format!(
                    "Ark video generation timed out after {} seconds",
                    max_wait.as_secs()
                )));
            }

            let response_json = self.get_json(&path).await?;
            let status = response_json
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let status = status.to_ascii_lowercase();

            match status.as_str() {
                "queued" | "running" => tokio::time::sleep(poll_interval).await,
                "succeeded" | "completed" => {
                    let video_url = response_json
                        .pointer("/content/video_url")
                        .or_else(|| response_json.pointer("/result/video_url"))
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| {
                            provider_error("Ark poll response missing video_url".to_string())
                        })?;
                    let last_frame_url = response_json
                        .pointer("/content/last_frame_url")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string);
                    let bytes = self.download_video(video_url).await?;
                    return Ok(VideoGenerationResponse {
                        video_data: BASE64.encode(bytes),
                        video_url: Some(video_url.to_string()),
                        last_frame_url,
                        model: response_json
                            .get("model")
                            .and_then(|value| value.as_str())
                            .unwrap_or(&request.model_id)
                            .to_string(),
                        operation_name: request.operation_name,
                        raw_response: Some(response_json),
                    });
                }
                "failed" | "cancelled" | "expired" => {
                    return Err(provider_error(format!(
                        "Ark video generation failed: {}",
                        response_json
                    )));
                }
                _ => tokio::time::sleep(poll_interval).await,
            }
        }
    }
}

impl ArkProvider {
    async fn get_json(&self, path: &str) -> AiResult<Value> {
        let response = self
            .client()?
            .get(self.url(path))
            .header(AUTHORIZATION, format!("Bearer {}", self.config.api_key))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(provider_error(format!(
                "Ark request failed: path={} status={} body={}",
                path, status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|error| AiFoundationError::Serialization(error.to_string()))
    }

    async fn download_video(&self, url: &str) -> AiResult<Vec<u8>> {
        let response = self
            .client()?
            .get(url)
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;
        if !response.status().is_success() {
            return Err(provider_error(format!(
                "Ark video download failed: status={} url={}",
                response.status(),
                url
            )));
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| AiFoundationError::Network(error.to_string()))
    }
}

fn output<T>(
    model_id: String,
    usage: Option<Usage>,
    raw: Option<Value>,
    data: T,
) -> AiProviderOutput<T> {
    AiProviderOutput {
        data,
        usage,
        provider: Provider::Ark,
        model_id,
        raw,
    }
}

#[async_trait]
impl ToolCallingProvider for ArkProvider {
    async fn tool_calling(
        &self,
        request: ProviderToolCallingRequest,
    ) -> AiResult<AiProviderOutput<ToolCallingResponse>> {
        let body =
            convert_vertex_tool_calling_payload_to_ark_body(&request.model_id, request.payload);
        let response_json = self.post_json("chat/completions", &body).await?;
        let usage = extract_openai_usage(&response_json);
        let data = extract_ark_tool_call(&response_json)
            .map(|(name, args)| ToolCallingResponse::ToolCall {
                name,
                args,
                usage: Some(usage.clone()),
                raw_part: None,
            })
            .or_else(|| {
                extract_ark_message_text(&response_json).map(|text| ToolCallingResponse::Text {
                    text,
                    usage: Some(usage.clone()),
                })
            })
            .ok_or_else(|| provider_error("Ark response missing text or tool_calls".to_string()))?;

        Ok(output(
            request.model_id,
            Some(usage),
            Some(response_json),
            data,
        ))
    }
}

fn provider_error(message: String) -> AiFoundationError {
    AiFoundationError::ProviderError {
        provider: Provider::Ark,
        message,
    }
}

fn normalize_role(role: &str) -> &str {
    match role.to_ascii_lowercase().as_str() {
        "assistant" | "model" => "assistant",
        "system" => "system",
        "tool" => "tool",
        _ => "user",
    }
}

fn convert_vertex_payload_to_ark_body(model_id: &str, payload: Value) -> Value {
    if payload.get("messages").is_some() {
        let mut body = payload;
        if body.get("model").is_none()
            && let Some(obj) = body.as_object_mut()
        {
            obj.insert("model".to_string(), Value::String(model_id.to_string()));
        }
        return body;
    }

    let mut messages = Vec::new();
    if let Some(system) = system_instruction_text(&payload) {
        messages.push(serde_json::json!({ "role": "system", "content": system }));
    }

    if let Some(contents) = payload.get("contents").and_then(|value| value.as_array()) {
        for content in contents {
            let role = normalize_role(
                content
                    .get("role")
                    .and_then(|value| value.as_str())
                    .unwrap_or("user"),
            );
            if let Some(content) = ark_chat_content_from_vertex_parts(
                content.get("parts").and_then(|value| value.as_array()),
            ) {
                messages.push(serde_json::json!({ "role": role, "content": content }));
            }
        }
    }

    if messages.is_empty()
        && let Some(prompt) = payload.get("prompt").and_then(|value| value.as_str())
    {
        messages.push(serde_json::json!({ "role": "user", "content": prompt }));
    }

    serde_json::json!({
        "model": model_id,
        "messages": messages,
    })
}

fn convert_vertex_tool_calling_payload_to_ark_body(model_id: &str, payload: Value) -> Value {
    let mut body = convert_vertex_payload_to_ark_body(model_id, payload.clone());

    if let Some(obj) = body.as_object_mut() {
        if let Some(tools) = convert_vertex_tools_to_openai_tools(payload.get("tools")) {
            obj.insert("tools".to_string(), tools);
            obj.insert(
                "tool_choice".to_string(),
                Value::String("required".to_string()),
            );
        }

        if let Some(temperature) = payload.pointer("/generationConfig/temperature") {
            obj.insert("temperature".to_string(), temperature.clone());
        }
        if let Some(max_tokens) = payload.pointer("/generationConfig/maxOutputTokens") {
            obj.insert("max_tokens".to_string(), max_tokens.clone());
        }
    }

    body
}

fn convert_vertex_tools_to_openai_tools(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mut openai_tools = Vec::new();

    for tool in tools {
        let Some(functions) = tool.get("functionDeclarations").and_then(Value::as_array) else {
            continue;
        };

        for function in functions {
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                continue;
            };
            openai_tools.push(serde_json::json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": function.get("description").and_then(Value::as_str).unwrap_or_default(),
                    "parameters": function.get("parameters").cloned().unwrap_or_else(|| serde_json::json!({
                        "type": "object",
                        "properties": {},
                    })),
                }
            }));
        }
    }

    if openai_tools.is_empty() {
        None
    } else {
        Some(Value::Array(openai_tools))
    }
}

fn system_instruction_text(payload: &Value) -> Option<String> {
    ["/systemInstruction/parts", "/system_instruction/parts"]
        .iter()
        .filter_map(|pointer| payload.pointer(pointer).and_then(Value::as_array))
        .flat_map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .next()
}

fn ark_chat_content_from_vertex_parts(parts: Option<&Vec<Value>>) -> Option<Value> {
    let parts = parts?;
    let mut content_items = Vec::new();
    let mut text_items = Vec::new();
    let mut has_media = false;

    for part in parts {
        if let Some(text) = part.get("text").and_then(Value::as_str).map(str::trim)
            && !text.is_empty()
        {
            text_items.push(text.to_string());
            content_items.push(serde_json::json!({
                "type": "text",
                "text": text,
            }));
        }

        if let Some(media) = vertex_media_part_to_ark_content(part) {
            has_media = true;
            content_items.push(media);
        }
    }

    if content_items.is_empty() {
        return None;
    }

    if !has_media {
        return Some(Value::String(text_items.join("\n\n")));
    }

    Some(Value::Array(content_items))
}

fn vertex_media_part_to_ark_content(part: &Value) -> Option<Value> {
    let inline_data = part.get("inlineData").or_else(|| part.get("inline_data"));
    if let Some(inline_data) = inline_data {
        let mime_type = inline_data
            .get("mimeType")
            .or_else(|| inline_data.get("mime_type"))
            .and_then(Value::as_str)?;
        let data = inline_data.get("data").and_then(Value::as_str)?;
        return ark_chat_media_content(mime_type, &format!("data:{};base64,{}", mime_type, data));
    }

    let file_data = part.get("fileData").or_else(|| part.get("file_data"));
    if let Some(file_data) = file_data {
        let mime_type = file_data
            .get("mimeType")
            .or_else(|| file_data.get("mime_type"))
            .and_then(Value::as_str)?;
        let uri = file_data
            .get("fileUri")
            .or_else(|| file_data.get("file_uri"))
            .and_then(Value::as_str)?;
        return ark_chat_media_content(mime_type, uri);
    }

    None
}

fn ark_chat_media_content(mime_type: &str, url: &str) -> Option<Value> {
    let mime_type = mime_type.to_ascii_lowercase();
    if mime_type.starts_with("image/") {
        return Some(serde_json::json!({
            "type": "image_url",
            "image_url": {
                "url": url,
            },
        }));
    }
    if mime_type.starts_with("video/") {
        return Some(serde_json::json!({
            "type": "video_url",
            "video_url": {
                "url": url,
            },
        }));
    }
    if mime_type.starts_with("audio/") {
        return Some(serde_json::json!({
            "type": "audio_url",
            "audio_url": {
                "url": url,
            },
        }));
    }

    None
}

fn extract_ark_message_text(response_json: &Value) -> Option<String> {
    let content = response_json.pointer("/choices/0/message/content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    let text = content
        .as_array()?
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.pointer("/content/0/text").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() { None } else { Some(text) }
}

fn extract_ark_tool_call(response_json: &Value) -> Option<(String, Value)> {
    let tool_call = response_json
        .pointer("/choices/0/message/tool_calls/0/function")
        .or_else(|| response_json.pointer("/choices/0/message/tool_call/function"))?;
    let name = tool_call.get("name")?.as_str()?.to_string();
    let arguments = tool_call.get("arguments")?;
    let args = if let Some(raw) = arguments.as_str() {
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({ "raw_arguments": raw }))
    } else {
        arguments.clone()
    };
    Some((name, args))
}

fn extract_openai_usage(response_json: &Value) -> Usage {
    Usage {
        prompt_tokens: response_json
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        completion_tokens: response_json
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        total_tokens: response_json
            .pointer("/usage/total_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        cached_tokens: None,
    }
}

fn extract_speech_input(payload: &Value) -> Option<String> {
    payload
        .get("input")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            payload
                .get("prompt")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .or_else(|| {
            payload
                .pointer("/instances/0/prompt")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .or_else(|| {
            let contents = payload.get("contents")?;
            if let Some(parts) = contents.get("parts") {
                return text_from_vertex_parts(parts);
            }
            if let Some(array) = contents.as_array() {
                let text = array
                    .iter()
                    .filter_map(|content| content.get("parts").and_then(text_from_vertex_parts))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                return if text.is_empty() { None } else { Some(text) };
            }
            None
        })
}

fn extract_text_prompt_from_contents(payload: &Value) -> Option<String> {
    let contents = payload.get("contents")?.as_array()?;
    let text = contents
        .iter()
        .filter_map(|content| content.get("parts").and_then(text_from_vertex_parts))
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() { None } else { Some(text) }
}

fn text_from_vertex_parts(parts: &Value) -> Option<String> {
    if let Some(text) = parts.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let text = parts
        .as_array()?
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() { None } else { Some(text) }
}

fn extract_speech_voice(payload: &Value) -> Option<&str> {
    payload
        .pointer("/generationConfig/speechConfig/voiceConfig/prebuiltVoiceConfig/voiceName")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .pointer("/generationConfig/speechConfig/multiSpeakerVoiceConfig/speakerVoiceConfigs/0/voiceConfig/prebuiltVoiceConfig/voiceName")
                .and_then(Value::as_str)
        })
        .or_else(|| payload.get("voice").and_then(Value::as_str))
}

fn normalize_ark_voice(voice: Option<&str>) -> &str {
    match voice.unwrap_or("alloy").to_ascii_lowercase().as_str() {
        "alloy" => "alloy",
        "echo" => "echo",
        "fable" => "fable",
        "onyx" => "onyx",
        "nova" => "nova",
        "shimmer" => "shimmer",
        _ => "alloy",
    }
}

fn convert_video_request_to_ark_body(request: &VideoGenerationRequest) -> AiResult<Value> {
    let mut content = vec![serde_json::json!({
        "type": "text",
        "text": request.prompt,
    })];

    for reference in &request.references {
        let mut item = match reference.kind {
            VideoReferenceKind::VideoReference => serde_json::json!({
                "type": "video_url",
                "video_url": {
                    "url": media_url(&reference.media)?,
                },
            }),
            VideoReferenceKind::AudioReference => serde_json::json!({
                "type": "audio_url",
                "audio_url": {
                    "url": media_url(&reference.media)?,
                },
            }),
            _ => serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": media_url(&reference.media)?,
                },
            }),
        };

        match reference.kind {
            VideoReferenceKind::FirstFrame => {
                item["role"] = Value::String("first_frame".to_string());
            }
            VideoReferenceKind::LastFrame => {
                item["role"] = Value::String("last_frame".to_string());
            }
            VideoReferenceKind::ProductReference
            | VideoReferenceKind::CharacterReference
            | VideoReferenceKind::EnvironmentReference
            | VideoReferenceKind::StyleReference
                if request.mode == VideoGenerationMode::ImageReferenceToVideo
                    && request.model.model_id == "doubao-seedance-1-0-lite-i2v-250428" =>
            {
                item["role"] = Value::String("reference_image".to_string());
            }
            _ => {}
        }

        content.push(item);
    }

    let mut body = serde_json::json!({
        "model": request.model.model_id,
        "content": content,
    });

    if let Some(ratio) = &request.output.ratio {
        body["ratio"] = Value::String(ratio.clone());
    }
    if let Some(duration) = request.output.duration_seconds {
        body["duration"] = Value::from(duration);
    }
    if let Some(frames) = request.output.frames {
        body["frames"] = Value::from(frames);
    }
    if let Some(resolution) = &request.output.resolution {
        body["resolution"] = Value::String(resolution.clone());
    }
    if let Some(seed) = request.output.seed {
        body["seed"] = Value::from(seed);
    }
    if let Some(camera_fixed) = request.output.camera_fixed {
        body["camera_fixed"] = Value::Bool(camera_fixed);
    }
    if let Some(watermark) = request.output.watermark {
        body["watermark"] = Value::Bool(watermark);
    }
    if let Some(generate_audio) = request.output.generate_audio {
        body["generate_audio"] = Value::Bool(generate_audio);
    }
    if let Some(return_last_frame) = request.output.return_last_frame {
        body["return_last_frame"] = Value::Bool(return_last_frame);
    }
    if let Some(service_tier) = &request.output.service_tier {
        body["service_tier"] = Value::String(service_tier.clone());
    }

    Ok(body)
}

fn media_url(media: &MediaInput) -> AiResult<String> {
    match media {
        MediaInput::Url { url } => Ok(url.clone()),
        MediaInput::GcsUri { uri } => Ok(uri.clone()),
        MediaInput::Base64 { mime_type, data } => Ok(format!("data:{};base64,{}", mime_type, data)),
        MediaInput::Bytes { mime_type, data } => {
            Ok(format!("data:{};base64,{}", mime_type, BASE64.encode(data)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::types::{VideoOutputSpec, VideoReference, VideoReferenceRequirement};

    fn image(kind: VideoReferenceKind) -> VideoReference {
        VideoReference {
            kind,
            media: MediaInput::Base64 {
                mime_type: "image/png".to_string(),
                data: "abc".to_string(),
            },
            requirement: VideoReferenceRequirement::Required,
            label: None,
        }
    }

    #[test]
    fn maps_lite_i2v_image_references_to_reference_image_role() {
        let request = VideoGenerationRequest::new(
            "doubao-seedance-1-0-lite-i2v-250428",
            VideoGenerationMode::ImageReferenceToVideo,
            "make a product shot",
        )
        .with_references(vec![image(VideoReferenceKind::ProductReference)]);

        let body = convert_video_request_to_ark_body(&request).unwrap();
        assert_eq!(
            body.pointer("/content/1/role")
                .and_then(|value| value.as_str()),
            Some("reference_image")
        );
    }

    #[test]
    fn maps_first_last_frame_roles_explicitly() {
        let request = VideoGenerationRequest::new(
            "doubao-seedance-2-0-260128",
            VideoGenerationMode::FirstLastFrameToVideo,
            "connect the frames",
        )
        .with_references(vec![
            image(VideoReferenceKind::FirstFrame),
            image(VideoReferenceKind::LastFrame),
        ]);

        let body = convert_video_request_to_ark_body(&request).unwrap();
        assert_eq!(
            body.pointer("/content/1/role")
                .and_then(|value| value.as_str()),
            Some("first_frame")
        );
        assert_eq!(
            body.pointer("/content/2/role")
                .and_then(|value| value.as_str()),
            Some("last_frame")
        );
    }

    #[test]
    fn maps_seedance_2_multimodal_references_without_image_role() {
        let request = VideoGenerationRequest::new(
            "doubao-seedance-2-0-260128",
            VideoGenerationMode::MultimodalReferenceToVideo,
            "use the visual and motion references",
        )
        .with_output(VideoOutputSpec {
            return_last_frame: Some(true),
            service_tier: Some("default".to_string()),
            ..Default::default()
        })
        .with_references(vec![
            image(VideoReferenceKind::ProductReference),
            VideoReference {
                kind: VideoReferenceKind::VideoReference,
                media: MediaInput::Url {
                    url: "https://example.test/ref.mp4".to_string(),
                },
                requirement: VideoReferenceRequirement::Optional,
                label: None,
            },
        ]);

        let body = convert_video_request_to_ark_body(&request).unwrap();
        assert!(body.pointer("/content/1/role").is_none());
        assert_eq!(
            body.pointer("/content/2/type")
                .and_then(|value| value.as_str()),
            Some("video_url")
        );
        assert_eq!(
            body.get("return_last_frame")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn converts_snake_case_system_instruction_for_storyboard_payload() {
        let body = convert_vertex_payload_to_ark_body(
            "ep-storyboard-test",
            serde_json::json!({
                "system_instruction": {
                    "parts": [{ "text": "Return storyboard markup only." }]
                },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": "Creative Brief: premium laptop ad" }]
                }]
            }),
        );

        assert_eq!(
            body.get("model").and_then(Value::as_str),
            Some("ep-storyboard-test")
        );
        assert_eq!(
            body.pointer("/messages/0/role").and_then(Value::as_str),
            Some("system")
        );
        assert_eq!(
            body.pointer("/messages/0/content").and_then(Value::as_str),
            Some("Return storyboard markup only.")
        );
        assert_eq!(
            body.pointer("/messages/1/role").and_then(Value::as_str),
            Some("user")
        );
        assert_eq!(
            body.pointer("/messages/1/content").and_then(Value::as_str),
            Some("Creative Brief: premium laptop ad")
        );
    }

    #[test]
    fn converts_inline_image_and_video_parts_to_ark_chat_content() {
        let body = convert_vertex_payload_to_ark_body(
            "ep-multimodal-test",
            serde_json::json!({
                "systemInstruction": {
                    "parts": [{ "text": "Analyze references." }]
                },
                "contents": [{
                    "role": "user",
                    "parts": [
                        { "text": "Product Image" },
                        {
                            "inlineData": {
                                "mimeType": "image/png",
                                "data": "image-base64"
                            }
                        },
                        { "text": "Reference Video" },
                        {
                            "inlineData": {
                                "mimeType": "video/mp4",
                                "data": "video-base64"
                            }
                        }
                    ]
                }]
            }),
        );

        assert_eq!(
            body.pointer("/messages/1/content/0/type")
                .and_then(Value::as_str),
            Some("text")
        );
        assert_eq!(
            body.pointer("/messages/1/content/1/type")
                .and_then(Value::as_str),
            Some("image_url")
        );
        assert_eq!(
            body.pointer("/messages/1/content/1/image_url/url")
                .and_then(Value::as_str),
            Some("data:image/png;base64,image-base64")
        );
        assert_eq!(
            body.pointer("/messages/1/content/2/type")
                .and_then(Value::as_str),
            Some("text")
        );
        assert_eq!(
            body.pointer("/messages/1/content/3/type")
                .and_then(Value::as_str),
            Some("video_url")
        );
        assert_eq!(
            body.pointer("/messages/1/content/3/video_url/url")
                .and_then(Value::as_str),
            Some("data:video/mp4;base64,video-base64")
        );
    }

    #[test]
    fn extracts_string_and_array_message_content() {
        let string_response = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "plain text"
                }
            }]
        });
        assert_eq!(
            extract_ark_message_text(&string_response).as_deref(),
            Some("plain text")
        );

        let array_response = serde_json::json!({
            "choices": [{
                "message": {
                    "content": [
                        { "type": "text", "text": "hello " },
                        { "type": "text", "text": "world" }
                    ]
                }
            }]
        });
        assert_eq!(
            extract_ark_message_text(&array_response).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn converts_vertex_tool_declarations_to_openai_tools() {
        let body = convert_vertex_tool_calling_payload_to_ark_body(
            "ep-agent-test",
            serde_json::json!({
                "systemInstruction": {
                    "parts": [{ "text": "Use tools." }]
                },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": "Generate a storyboard" }]
                }],
                "tools": [{
                    "functionDeclarations": [{
                        "name": "generate_storyboard",
                        "description": "Generate storyboard",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "creative_brief": { "type": "string" }
                            },
                            "required": ["creative_brief"]
                        }
                    }]
                }],
                "toolConfig": {
                    "functionCallingConfig": {
                        "mode": "ANY"
                    }
                },
                "generationConfig": {
                    "temperature": 0.2,
                    "maxOutputTokens": 4096
                }
            }),
        );

        assert_eq!(
            body.pointer("/tools/0/type").and_then(Value::as_str),
            Some("function")
        );
        assert_eq!(
            body.pointer("/tools/0/function/name")
                .and_then(Value::as_str),
            Some("generate_storyboard")
        );
        assert_eq!(
            body.get("tool_choice").and_then(Value::as_str),
            Some("required")
        );
        assert_eq!(body.get("temperature").and_then(Value::as_f64), Some(0.2));
        assert_eq!(body.get("max_tokens").and_then(Value::as_i64), Some(4096));
    }

    #[test]
    fn extracts_ark_tool_call_arguments() {
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "generate_storyboard",
                            "arguments": "{\"creative_brief\":\"premium ad\"}"
                        }
                    }]
                }
            }]
        });

        let (name, args) = extract_ark_tool_call(&response).unwrap();
        assert_eq!(name, "generate_storyboard");
        assert_eq!(args["creative_brief"], "premium ad");
    }

    #[test]
    fn extracts_speech_input_and_normalizes_gemini_voice() {
        let payload = serde_json::json!({
            "contents": {
                "role": "user",
                "parts": {
                    "text": "calmly: hello"
                }
            },
            "generationConfig": {
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": "Kore"
                        }
                    }
                }
            }
        });

        assert_eq!(
            extract_speech_input(&payload).as_deref(),
            Some("calmly: hello")
        );
        assert_eq!(normalize_ark_voice(extract_speech_voice(&payload)), "alloy");
    }

    #[test]
    fn extracts_image_prompt_from_vertex_contents() {
        let payload = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [
                    { "text": "Generate a product image" },
                    {
                        "inlineData": {
                            "mimeType": "image/png",
                            "data": "abc"
                        }
                    }
                ]
            }]
        });

        assert_eq!(
            extract_text_prompt_from_contents(&payload).as_deref(),
            Some("Generate a product image")
        );
    }
}
