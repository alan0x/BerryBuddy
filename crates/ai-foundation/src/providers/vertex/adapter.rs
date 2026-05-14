use async_trait::async_trait;
use base64::Engine;
use gcp_auth::TokenProvider;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::error::{AiFoundationError, AiResult};
use crate::generic::types::{
    VideoGenerationMode, VideoGenerationRequest, VideoReference, VideoReferenceKind,
};
use crate::media::MediaInput;
use crate::operation::Provider;
use crate::providers::traits::audio::{AudioProvider, ProviderAudioRequest};
use crate::providers::traits::image::{ImageProvider, ProviderImageRequest};
use crate::providers::traits::text::{ProviderTextRequest, TextProvider};
use crate::providers::traits::tool_calling::{ProviderToolCallingRequest, ToolCallingProvider};
use crate::providers::traits::video::{
    ProviderVideoPollRequest, ProviderVideoStartRequest, VideoProvider,
};
use crate::providers::vertex::config::{VertexAuth, VertexProviderConfig};
use crate::providers::vertex::gcs_staging::VertexGcsStaging;
use crate::types::{
    AiProviderOutput, AudioGenerationResponse, ImageGenerationResponse, SpeechGenerationResponse,
    TextGenerationResponse, ToolCallingResponse, Usage, VideoGenerationResponse,
};

#[derive(Debug, Clone)]
pub struct VertexProvider {
    config: VertexProviderConfig,
    gcs_staging: Option<VertexGcsStaging>,
}

impl VertexProvider {
    pub fn new(config: VertexProviderConfig) -> Self {
        let gcs_staging = config.gcs.clone().map(VertexGcsStaging::new);
        Self {
            config,
            gcs_staging,
        }
    }

    pub fn gcs_staging(&self) -> Option<&VertexGcsStaging> {
        self.gcs_staging.as_ref()
    }

    fn build_url(&self, model_id: &str, api: &str) -> String {
        let api_version = vertex_api_version(api);
        let host = vertex_api_host(&self.config.location);
        format!(
            "https://{}/{}/projects/{}/locations/{}/publishers/google/models/{}:{}",
            host, api_version, self.config.project_id, self.config.location, model_id, api
        )
    }

    async fn client(&self) -> AiResult<reqwest::Client> {
        match &self.config.auth {
            VertexAuth::AccessToken(token) => client_with_token(token, self.config.timeout_secs),
            VertexAuth::ServiceAccountJson(json) => {
                let token = service_account_token(json).await?;
                client_with_token(&token, self.config.timeout_secs)
            }
        }
    }

    async fn post_json(&self, model_id: &str, api: &str, payload: &Value) -> AiResult<Value> {
        let client = self.client().await?;
        let url = self.build_url(model_id, api);

        const MAX_RETRIES: u32 = 10;
        const MAX_BACKOFF: Duration = Duration::from_secs(60);
        let mut backoff = Duration::from_secs(5);

        for attempt in 0..=MAX_RETRIES {
            let response = match client.post(&url).json(payload).send().await {
                Ok(response) => response,
                Err(error) if is_retryable_reqwest_error(&error) && attempt < MAX_RETRIES => {
                    tracing::warn!(
                        provider = "vertex",
                        api,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        wait_secs = backoff.as_secs(),
                        error = %error,
                        "transient Vertex request error; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
                Err(error) => return Err(AiFoundationError::Network(error.to_string())),
            };

            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let body = response.text().await.unwrap_or_default();
                tracing::warn!(
                    provider = "vertex",
                    api,
                    attempt = attempt + 1,
                    max_retries = MAX_RETRIES,
                    wait_secs = backoff.as_secs(),
                    body = %body.chars().take(500).collect::<String>(),
                    "Vertex rate limited request; retrying"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: format!(
                        "Vertex request failed: api={} status={} body={}",
                        api, status, body
                    ),
                });
            }

            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(error) if is_retryable_reqwest_error(&error) && attempt < MAX_RETRIES => {
                    tracing::warn!(
                        provider = "vertex",
                        api,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        wait_secs = backoff.as_secs(),
                        error = %error,
                        "transient Vertex response body error; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
                Err(error) => {
                    return Err(AiFoundationError::Serialization(format!(
                        "Vertex response body read failed: {}",
                        error
                    )));
                }
            };

            return serde_json::from_slice(&bytes).map_err(|error| {
                let preview = String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(500)
                    .collect::<String>();
                AiFoundationError::Serialization(format!(
                    "Vertex response JSON parse failed: api={} error={} body_preview={}",
                    api, error, preview
                ))
            });
        }

        unreachable!()
    }
}

#[async_trait]
impl TextProvider for VertexProvider {
    async fn text_to_text(
        &self,
        request: ProviderTextRequest,
    ) -> AiResult<AiProviderOutput<TextGenerationResponse>> {
        let response_json = self
            .post_json(&request.model_id, "generateContent", &request.payload)
            .await?;
        let text = extract_text_from_response(&response_json).ok_or_else(|| {
            AiFoundationError::ProviderError {
                provider: Provider::Vertex,
                message: "No text in Vertex response".to_string(),
            }
        })?;
        let usage = extract_usage_from_response(&response_json);
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
impl ImageProvider for VertexProvider {
    async fn text_to_image(
        &self,
        request: ProviderImageRequest,
    ) -> AiResult<AiProviderOutput<ImageGenerationResponse>> {
        let api = if request.model_id.to_ascii_lowercase().contains("imagen") {
            "predict"
        } else {
            "generateContent"
        };
        let response_json = self
            .post_json(&request.model_id, api, &request.payload)
            .await?;
        let (image_data, mime_type) =
            extract_image_from_response(&response_json).ok_or_else(|| {
                AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: "No image data in Vertex response".to_string(),
                }
            })?;
        let usage = extract_usage_from_response(&response_json);
        let response = ImageGenerationResponse {
            image_data,
            mime_type,
            usage: usage.clone(),
            model: request.model_id.clone(),
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
impl AudioProvider for VertexProvider {
    async fn text_to_audio(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<AudioGenerationResponse>> {
        let response_json = self
            .post_json(&request.model_id, "predict", &request.payload)
            .await?;
        let audio_data = response_json
            .pointer("/predictions/0/bytesBase64Encoded")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiFoundationError::ProviderError {
                provider: Provider::Vertex,
                message: "No audio data in Vertex response".to_string(),
            })?
            .to_string();
        let response = AudioGenerationResponse {
            audio_data,
            mime_type: "audio/wav".to_string(),
            model: request.model_id.clone(),
        };
        Ok(output(
            request.model_id,
            None,
            Some(response_json),
            response,
        ))
    }

    async fn text_to_speech(
        &self,
        request: ProviderAudioRequest,
    ) -> AiResult<AiProviderOutput<SpeechGenerationResponse>> {
        let response_json = self
            .post_json(&request.model_id, "generateContent", &request.payload)
            .await?;
        let audio_data = response_json
            .pointer("/candidates/0/content/parts/0/inlineData/data")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AiFoundationError::ProviderError {
                provider: Provider::Vertex,
                message: "No speech audio data in Vertex response".to_string(),
            })?
            .to_string();
        let usage = extract_usage_from_response(&response_json);
        let response = SpeechGenerationResponse {
            audio_data,
            mime_type: "audio/pcm".to_string(),
            sample_rate: 24000,
            usage: usage.clone(),
            model: request.model_id.clone(),
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
impl VideoProvider for VertexProvider {
    async fn start_video_generation(&self, request: ProviderVideoStartRequest) -> AiResult<String> {
        let payload = convert_video_request_to_vertex_payload(&request.request)?;
        let model_id = request.request.model.model_id.clone();
        let response_json = self
            .post_json(&model_id, "predictLongRunning", &payload)
            .await?;
        response_json
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
            .ok_or_else(|| AiFoundationError::ProviderError {
                provider: Provider::Vertex,
                message: "No operation name in Vertex video response".to_string(),
            })
    }

    async fn poll_video_generation(
        &self,
        request: ProviderVideoPollRequest,
    ) -> AiResult<VideoGenerationResponse> {
        let poll_interval = Duration::from_secs(request.poll_interval_secs.unwrap_or(10));
        let max_wait = Duration::from_secs(request.max_wait_secs.unwrap_or(600));
        let start_time = Instant::now();
        let client = self.client().await?;
        let url = self.build_url(&request.model_id, "fetchPredictOperation");
        let payload = serde_json::json!({ "operationName": request.operation_name });

        loop {
            if start_time.elapsed() > max_wait {
                return Err(AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: format!(
                        "Vertex video generation timed out after {} seconds",
                        max_wait.as_secs()
                    ),
                });
            }

            let response = client
                .post(&url)
                .json(&payload)
                .send()
                .await
                .map_err(|error| AiFoundationError::Network(error.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: format!(
                        "Vertex video polling failed: status={} body={}",
                        status, body
                    ),
                });
            }

            let response_json: Value = response
                .json()
                .await
                .map_err(|error| AiFoundationError::Serialization(error.to_string()))?;

            let done = response_json
                .get("done")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if !done {
                tokio::time::sleep(poll_interval).await;
                continue;
            }

            if let Some(error) = response_json.get("error") {
                let code = error
                    .get("code")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0);
                let message = error
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown error");
                return Err(AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: format!(
                        "Vertex video generation operation failed: code={} message={}",
                        code, message
                    ),
                });
            }

            let response =
                response_json
                    .get("response")
                    .ok_or_else(|| AiFoundationError::ProviderError {
                        provider: Provider::Vertex,
                        message: "Vertex operation done but no response field".to_string(),
                    })?;
            let rai_filtered_count = response
                .get("raiMediaFilteredCount")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            let videos = response
                .get("videos")
                .and_then(|value| value.as_array())
                .ok_or_else(|| AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: if rai_filtered_count > 0 {
                        format!(
                            "All {} generated videos were filtered by Responsible AI policies",
                            rai_filtered_count
                        )
                    } else {
                        "No videos in Vertex polling response".to_string()
                    },
                })?;
            let first_video = videos
                .first()
                .ok_or_else(|| AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: if rai_filtered_count > 0 {
                        format!(
                            "All {} generated videos were filtered by Responsible AI policies",
                            rai_filtered_count
                        )
                    } else {
                        "Empty videos in Vertex polling response".to_string()
                    },
                })?;
            let video_data = first_video
                .get("bytesBase64Encoded")
                .or_else(|| first_video.get("gcsUri"))
                .and_then(|value| value.as_str())
                .ok_or_else(|| AiFoundationError::ProviderError {
                    provider: Provider::Vertex,
                    message: "Video entry has neither bytesBase64Encoded nor gcsUri".to_string(),
                })?
                .to_string();

            return Ok(VideoGenerationResponse {
                video_data,
                video_url: None,
                last_frame_url: None,
                model: request.model_id,
                operation_name: request.operation_name,
                raw_response: Some(response_json),
            });
        }
    }
}

fn convert_video_request_to_vertex_payload(request: &VideoGenerationRequest) -> AiResult<Value> {
    let mut instance = serde_json::json!({
        "prompt": request.prompt,
    });

    match request.mode {
        VideoGenerationMode::TextToVideo => {}
        VideoGenerationMode::FirstFrameToVideo => {
            let first_frame = request
                .references
                .iter()
                .find(|reference| reference.kind == VideoReferenceKind::FirstFrame)
                .ok_or_else(|| {
                    AiFoundationError::InvalidRequest(
                        "First-frame video request requires first_frame reference.".to_string(),
                    )
                })?;
            instance["image"] = vertex_image(&first_frame.media)?;
        }
        VideoGenerationMode::FirstLastFrameToVideo => {
            let first_frame = request
                .references
                .iter()
                .find(|reference| reference.kind == VideoReferenceKind::FirstFrame)
                .ok_or_else(|| {
                    AiFoundationError::InvalidRequest(
                        "First-last-frame video request requires first_frame reference."
                            .to_string(),
                    )
                })?;
            let last_frame = request
                .references
                .iter()
                .find(|reference| reference.kind == VideoReferenceKind::LastFrame)
                .ok_or_else(|| {
                    AiFoundationError::InvalidRequest(
                        "First-last-frame video request requires last_frame reference.".to_string(),
                    )
                })?;
            instance["image"] = vertex_image(&first_frame.media)?;
            instance["lastFrame"] = vertex_image(&last_frame.media)?;
        }
        VideoGenerationMode::ImageReferenceToVideo
        | VideoGenerationMode::MultimodalReferenceToVideo => {
            let reference_images = request
                .references
                .iter()
                .filter(|reference| {
                    matches!(
                        reference.kind,
                        VideoReferenceKind::ProductReference
                            | VideoReferenceKind::CharacterReference
                            | VideoReferenceKind::EnvironmentReference
                            | VideoReferenceKind::StyleReference
                    )
                })
                .map(vertex_reference_image)
                .collect::<AiResult<Vec<_>>>()?;

            if !reference_images.is_empty() {
                instance["referenceImages"] = Value::Array(reference_images);
            }
        }
    }

    let mut payload = serde_json::json!({
        "instances": [instance],
        "parameters": {},
    });

    if let Some(ratio) = &request.output.ratio {
        payload["parameters"]["aspectRatio"] = Value::String(ratio.clone());
    }
    if let Some(duration) = request.output.duration_seconds {
        payload["parameters"]["durationSeconds"] = Value::from(duration);
    }
    if let Some(resolution) = &request.output.resolution {
        payload["parameters"]["resolution"] = Value::String(resolution.clone());
    }
    if let Some(seed) = request.output.seed {
        payload["parameters"]["seed"] = Value::from(seed);
    }
    if let Some(generate_audio) = request.output.generate_audio {
        payload["parameters"]["generateAudio"] = Value::Bool(generate_audio);
    }
    if let Some(watermark) = request.output.watermark {
        payload["parameters"]["watermark"] = Value::Bool(watermark);
    }

    Ok(payload)
}

fn vertex_reference_image(reference: &VideoReference) -> AiResult<Value> {
    let reference_type = match reference.kind {
        VideoReferenceKind::StyleReference => "style",
        _ => "asset",
    };

    let image = vertex_image(&reference.media)?;

    Ok(serde_json::json!({
        "image": image,
        "referenceType": reference_type,
    }))
}

fn vertex_image(media: &MediaInput) -> AiResult<Value> {
    match media {
        MediaInput::Base64 { mime_type, data } => Ok(serde_json::json!({
            "bytesBase64Encoded": data,
            "mimeType": mime_type,
        })),
        MediaInput::Bytes { mime_type, data } => Ok(serde_json::json!({
            "bytesBase64Encoded": base64::engine::general_purpose::STANDARD.encode(data),
            "mimeType": mime_type,
        })),
        MediaInput::Url { url } if url.starts_with("gs://") => Ok(serde_json::json!({
            "gcsUri": url,
        })),
        MediaInput::Url { url } => Err(AiFoundationError::InvalidRequest(format!(
            "Vertex video image references require base64 data or gs:// URIs, got URL {}.",
            url
        ))),
        MediaInput::GcsUri { uri } => Ok(serde_json::json!({
            "gcsUri": uri,
        })),
    }
}

#[async_trait]
impl ToolCallingProvider for VertexProvider {
    async fn tool_calling(
        &self,
        request: ProviderToolCallingRequest,
    ) -> AiResult<AiProviderOutput<ToolCallingResponse>> {
        let response_json = self
            .post_json(&request.model_id, "generateContent", &request.payload)
            .await?;
        let usage = extract_usage_from_response(&response_json);
        let data = extract_function_call_from_response(&response_json)
            .map(|(name, args, raw_part)| ToolCallingResponse::ToolCall {
                name,
                args,
                usage: Some(usage.clone()),
                raw_part: Some(raw_part),
            })
            .or_else(|| {
                extract_text_from_response(&response_json).map(|text| ToolCallingResponse::Text {
                    text,
                    usage: Some(usage.clone()),
                })
            })
            .ok_or_else(|| AiFoundationError::ProviderError {
                provider: Provider::Vertex,
                message: "No text or functionCall in Vertex response".to_string(),
            })?;
        Ok(output(
            request.model_id,
            Some(usage),
            Some(response_json),
            data,
        ))
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
        provider: Provider::Vertex,
        model_id,
        raw,
    }
}

fn vertex_api_version(api: &str) -> &'static str {
    if api == "generateContent" {
        "v1beta1"
    } else {
        "v1"
    }
}

fn vertex_api_host(location: &str) -> String {
    if location.eq_ignore_ascii_case("global") {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", location)
    }
}

fn extract_usage_from_response(response: &Value) -> Usage {
    let Some(usage) = response.get("usageMetadata") else {
        return Usage::default();
    };

    Usage {
        prompt_tokens: usage
            .get("promptTokenCount")
            .and_then(|value| value.as_i64())
            .unwrap_or(0),
        completion_tokens: usage
            .get("candidatesTokenCount")
            .and_then(|value| value.as_i64())
            .unwrap_or(0),
        total_tokens: usage
            .get("totalTokenCount")
            .and_then(|value| value.as_i64())
            .unwrap_or(0),
        cached_tokens: usage
            .get("cachedContentTokenCount")
            .and_then(|value| value.as_i64()),
    }
}

fn extract_text_from_response(response: &Value) -> Option<String> {
    response
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn extract_function_call_from_response(response: &Value) -> Option<(String, Value, Value)> {
    let part = response
        .pointer("/candidates/0/content/parts")?
        .as_array()?
        .iter()
        .find(|part| part.get("functionCall").is_some())?;
    let function_call = part.get("functionCall")?;
    let name = function_call.get("name")?.as_str()?.to_string();
    let args = function_call
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Some((name, args, part.clone()))
}

fn extract_image_from_response(response: &Value) -> Option<(String, String)> {
    if let Some(parts) = response
        .pointer("/candidates/0/content/parts")
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if let Some(inline_data) = part.get("inlineData") {
                let mime_type = inline_data
                    .get("mimeType")
                    .and_then(|value| value.as_str())
                    .unwrap_or("image/png");
                let data = inline_data.get("data").and_then(|value| value.as_str())?;
                return Some((data.to_string(), mime_type.to_string()));
            }
        }
    }

    response
        .pointer("/predictions/0/bytesBase64Encoded")
        .and_then(|value| value.as_str())
        .map(|data| (data.to_string(), "image/png".to_string()))
}

fn is_retryable_reqwest_error(error: &reqwest::Error) -> bool {
    error.is_connect()
        || error.is_request()
        || error.is_timeout()
        || error.is_body()
        || error.is_decode()
}

fn client_with_token(token: &str, timeout_secs: u64) -> AiResult<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {}", token).parse().map_err(|error| {
            AiFoundationError::Network(format!("invalid authorization header: {}", error))
        })?,
    );
    headers.insert(
        CONTENT_TYPE,
        "application/json".parse().map_err(|error| {
            AiFoundationError::Network(format!("invalid content-type header: {}", error))
        })?,
    );
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .default_headers(headers)
        .build()
        .map_err(|error| AiFoundationError::Network(error.to_string()))
}

async fn service_account_token(service_account_json: &str) -> AiResult<String> {
    install_rustls_provider();
    let service_account = gcp_auth::CustomServiceAccount::from_json(service_account_json)
        .map_err(|error| AiFoundationError::Storage(error.to_string()))?;
    let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
    let token = service_account
        .token(scopes)
        .await
        .map_err(|error| AiFoundationError::Storage(error.to_string()))?;
    Ok(token.as_str().to_string())
}

fn install_rustls_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::ring::default_provider(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::types::{
        VideoGenerationMode, VideoGenerationRequest, VideoReference, VideoReferenceKind,
        VideoReferenceRequirement,
    };
    use crate::providers::vertex::config::VertexAuth;

    fn provider(location: &str) -> VertexProvider {
        VertexProvider::new(VertexProviderConfig {
            project_id: "griseo".to_string(),
            location: location.to_string(),
            auth: VertexAuth::AccessToken("token".to_string()),
            timeout_secs: 660,
            gcs: None,
        })
    }

    #[test]
    fn generate_content_uses_global_vertex_host_and_v1beta1() {
        assert_eq!(
            provider("global").build_url("gemini-2.5-flash", "generateContent"),
            "https://aiplatform.googleapis.com/v1beta1/projects/griseo/locations/global/publishers/google/models/gemini-2.5-flash:generateContent"
        );
    }

    #[test]
    fn predict_uses_regional_vertex_host_and_v1() {
        assert_eq!(
            provider("us-central1").build_url("imagen-4.0-generate-001", "predict"),
            "https://us-central1-aiplatform.googleapis.com/v1/projects/griseo/locations/us-central1/publishers/google/models/imagen-4.0-generate-001:predict"
        );
    }

    #[test]
    fn preserves_raw_function_call_part_for_thought_signature() {
        let response = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        { "text": "I will create the project." },
                        {
                            "functionCall": {
                                "name": "create_project",
                                "args": { "name": "Launch film" }
                            },
                            "thoughtSignature": "opaque-signature"
                        },
                    ]
                }
            }]
        });

        let (name, args, raw_part) = extract_function_call_from_response(&response).unwrap();

        assert_eq!(name, "create_project");
        assert_eq!(args["name"], "Launch film");
        assert_eq!(raw_part["thoughtSignature"], "opaque-signature");
    }

    #[test]
    fn maps_base64_image_reference_to_vertex_reference_images() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-generate-001",
            VideoGenerationMode::ImageReferenceToVideo,
            "make a product shot",
        )
        .with_references(vec![VideoReference {
            kind: VideoReferenceKind::ProductReference,
            media: MediaInput::Base64 {
                mime_type: "image/png".to_string(),
                data: "abc".to_string(),
            },
            requirement: VideoReferenceRequirement::Required,
            label: None,
        }]);

        let payload = convert_video_request_to_vertex_payload(&request).unwrap();
        assert_eq!(
            payload.pointer("/instances/0/referenceImages/0/image/bytesBase64Encoded"),
            Some(&Value::String("abc".to_string()))
        );
    }

    #[test]
    fn maps_style_reference_to_vertex_style_reference_type() {
        let request = VideoGenerationRequest::new(
            "veo-2.0-generate-exp",
            VideoGenerationMode::ImageReferenceToVideo,
            "make a style shot",
        )
        .with_references(vec![VideoReference {
            kind: VideoReferenceKind::StyleReference,
            media: MediaInput::GcsUri {
                uri: "gs://bucket/style.png".to_string(),
            },
            requirement: VideoReferenceRequirement::Required,
            label: None,
        }]);

        let payload = convert_video_request_to_vertex_payload(&request).unwrap();
        assert_eq!(
            payload.pointer("/instances/0/referenceImages/0/referenceType"),
            Some(&Value::String("style".to_string()))
        );
    }

    #[test]
    fn maps_first_last_frames_to_vertex_image_fields() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-fast-generate-001",
            VideoGenerationMode::FirstLastFrameToVideo,
            "move from the first frame to the last frame",
        )
        .with_references(vec![
            VideoReference {
                kind: VideoReferenceKind::FirstFrame,
                media: MediaInput::GcsUri {
                    uri: "gs://bucket/first.png".to_string(),
                },
                requirement: VideoReferenceRequirement::Required,
                label: None,
            },
            VideoReference {
                kind: VideoReferenceKind::LastFrame,
                media: MediaInput::GcsUri {
                    uri: "gs://bucket/last.png".to_string(),
                },
                requirement: VideoReferenceRequirement::Required,
                label: None,
            },
        ]);

        let payload = convert_video_request_to_vertex_payload(&request).unwrap();
        assert_eq!(
            payload.pointer("/instances/0/image/gcsUri"),
            Some(&Value::String("gs://bucket/first.png".to_string()))
        );
        assert_eq!(
            payload.pointer("/instances/0/lastFrame/gcsUri"),
            Some(&Value::String("gs://bucket/last.png".to_string()))
        );
        assert!(payload.pointer("/instances/0/referenceImages").is_none());
    }

    #[test]
    fn rejects_plain_url_image_reference_for_vertex_video() {
        let request = VideoGenerationRequest::new(
            "veo-2.0-generate-exp",
            VideoGenerationMode::ImageReferenceToVideo,
            "make a product shot",
        )
        .with_references(vec![VideoReference {
            kind: VideoReferenceKind::ProductReference,
            media: MediaInput::Url {
                url: "https://example.test/image.png".to_string(),
            },
            requirement: VideoReferenceRequirement::Required,
            label: None,
        }]);

        assert!(convert_video_request_to_vertex_payload(&request).is_err());
    }
}
