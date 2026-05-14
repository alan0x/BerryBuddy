use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{AiFoundationError, AiResult};
use crate::generic::types::VideoGenerationRequest;
use crate::operation::{Operation, Provider};
use crate::providers::capability;
use crate::providers::model_registry;
use crate::providers::traits::audio::{AudioProvider, ProviderAudioRequest};
use crate::providers::traits::image::{ImageProvider, ProviderImageRequest};
use crate::providers::traits::realtime::RealtimeConversationProvider;
use crate::providers::traits::text::{ProviderTextRequest, TextProvider};
use crate::providers::traits::tool_calling::{ProviderToolCallingRequest, ToolCallingProvider};
use crate::providers::traits::video::{
    ProviderVideoPollRequest, ProviderVideoStartRequest, VideoProvider,
};
use crate::providers::video_capability;
use crate::realtime::{RealtimeConnectRequest, RealtimeConversationSession};
use crate::types::{
    AudioGenerationResponse, ImageGenerationResponse, SpeechGenerationResponse,
    TextGenerationResponse, ToolCallingResponse, VideoGenerationResponse,
};

#[derive(Default)]
pub struct ProviderRouter {
    text: HashMap<Provider, Arc<dyn TextProvider>>,
    image: HashMap<Provider, Arc<dyn ImageProvider>>,
    audio: HashMap<Provider, Arc<dyn AudioProvider>>,
    video: HashMap<Provider, Arc<dyn VideoProvider>>,
    tool_calling: HashMap<Provider, Arc<dyn ToolCallingProvider>>,
    realtime: HashMap<Provider, Arc<dyn RealtimeConversationProvider>>,
}

impl ProviderRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn TextProvider>,
    ) -> Self {
        self.text.insert(provider, adapter);
        self
    }

    pub fn with_image_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn ImageProvider>,
    ) -> Self {
        self.image.insert(provider, adapter);
        self
    }

    pub fn with_audio_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn AudioProvider>,
    ) -> Self {
        self.audio.insert(provider, adapter);
        self
    }

    pub fn with_video_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn VideoProvider>,
    ) -> Self {
        self.video.insert(provider, adapter);
        self
    }

    pub fn with_tool_calling_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn ToolCallingProvider>,
    ) -> Self {
        self.tool_calling.insert(provider, adapter);
        self
    }

    pub fn with_realtime_provider(
        mut self,
        provider: Provider,
        adapter: Arc<dyn RealtimeConversationProvider>,
    ) -> Self {
        self.realtime.insert(provider, adapter);
        self
    }

    pub async fn text_to_text(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation(model_id, payload, Operation::TextToText)
            .await
    }

    pub async fn text_to_text_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::TextToText,
        )
        .await
    }

    pub async fn multimodal_to_text(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation(model_id, payload, Operation::MultimodalToText)
            .await
    }

    pub async fn multimodal_to_text_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::MultimodalToText,
        )
        .await
    }

    pub async fn image_to_text(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation(model_id, payload, Operation::ImageToText)
            .await
    }

    pub async fn image_to_text_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::ImageToText,
        )
        .await
    }

    pub async fn video_to_text(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation(model_id, payload, Operation::VideoToText)
            .await
    }

    pub async fn video_to_text_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<TextGenerationResponse> {
        self.text_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::VideoToText,
        )
        .await
    }

    async fn text_generation(
        &self,
        model_id: &str,
        payload: Value,
        operation: Operation,
    ) -> AiResult<TextGenerationResponse> {
        let resolved = model_registry::resolve_model_id(model_id, operation);
        self.text_generation_with_provider(
            resolved.provider,
            resolved.model_id,
            payload,
            resolved.operation,
        )
        .await
    }

    async fn text_generation_with_provider(
        &self,
        provider: Provider,
        model_id: String,
        payload: Value,
        operation: Operation,
    ) -> AiResult<TextGenerationResponse> {
        capability::ensure_supports(provider, operation)?;
        let adapter = self.text_provider(provider)?;
        Ok(adapter
            .text_to_text(ProviderTextRequest { model_id, payload })
            .await?
            .data)
    }

    pub async fn text_to_image(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ImageGenerationResponse> {
        self.image_generation(model_id, payload, Operation::TextToImage)
            .await
    }

    pub async fn text_to_image_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ImageGenerationResponse> {
        self.image_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::TextToImage,
        )
        .await
    }

    pub async fn image_to_image(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ImageGenerationResponse> {
        self.image_generation(model_id, payload, Operation::ImageToImage)
            .await
    }

    pub async fn image_to_image_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ImageGenerationResponse> {
        self.image_generation_with_provider(
            provider,
            model_id.to_string(),
            payload,
            Operation::ImageToImage,
        )
        .await
    }

    async fn image_generation(
        &self,
        model_id: &str,
        payload: Value,
        operation: Operation,
    ) -> AiResult<ImageGenerationResponse> {
        let resolved = model_registry::resolve_model_id(model_id, operation);
        self.image_generation_with_provider(
            resolved.provider,
            resolved.model_id,
            payload,
            resolved.operation,
        )
        .await
    }

    async fn image_generation_with_provider(
        &self,
        provider: Provider,
        model_id: String,
        payload: Value,
        operation: Operation,
    ) -> AiResult<ImageGenerationResponse> {
        capability::ensure_supports(provider, operation)?;
        let adapter = self.image_provider(provider)?;
        Ok(adapter
            .text_to_image(ProviderImageRequest { model_id, payload })
            .await?
            .data)
    }

    pub async fn text_to_audio(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<AudioGenerationResponse> {
        let resolved = model_registry::resolve_model_id(model_id, Operation::TextToAudio);
        self.text_to_audio_with_provider(resolved.provider, &resolved.model_id, payload)
            .await
    }

    pub async fn text_to_audio_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<AudioGenerationResponse> {
        capability::ensure_supports(provider, Operation::TextToAudio)?;
        let adapter = self.audio_provider(provider)?;
        Ok(adapter
            .text_to_audio(ProviderAudioRequest {
                model_id: model_id.to_string(),
                payload,
            })
            .await?
            .data)
    }

    pub async fn text_to_speech(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<SpeechGenerationResponse> {
        let resolved = model_registry::resolve_model_id(model_id, Operation::TextToAudio);
        self.text_to_speech_with_provider(resolved.provider, &resolved.model_id, payload)
            .await
    }

    pub async fn text_to_speech_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<SpeechGenerationResponse> {
        capability::ensure_supports(provider, Operation::TextToAudio)?;
        let adapter = self.audio_provider(provider)?;
        Ok(adapter
            .text_to_speech(ProviderAudioRequest {
                model_id: model_id.to_string(),
                payload,
            })
            .await?
            .data)
    }

    pub async fn run_video_generation(
        &self,
        request: VideoGenerationRequest,
    ) -> AiResult<VideoGenerationResponse> {
        let model_id = request.model.model_id.clone();
        let poll_interval_secs = request.poll_interval_secs;
        let max_wait_secs = request.max_wait_secs;
        let operation_name = self.start_video_generation_request(request).await?;
        self.poll_video_generation(
            &model_id,
            &operation_name,
            poll_interval_secs,
            max_wait_secs,
            Operation::VideoGeneration,
        )
        .await
    }

    pub async fn start_video_generation_request(
        &self,
        request: VideoGenerationRequest,
    ) -> AiResult<String> {
        let resolved = model_registry::resolve_model(request.model.clone());
        let mut request = request;
        request.model.provider = Some(resolved.provider);
        request.model.model_id = resolved.model_id;
        request.model.operation = Operation::VideoGeneration;
        self.start_video_generation_request_with_provider(resolved.provider, request)
            .await
    }

    pub async fn start_video_generation_request_with_provider(
        &self,
        provider: Provider,
        mut request: VideoGenerationRequest,
    ) -> AiResult<String> {
        capability::ensure_supports(provider, Operation::VideoGeneration)?;
        request.model.provider = Some(provider);
        request.model.operation = Operation::VideoGeneration;
        video_capability::ensure_video_request_supported(provider, &request)?;
        let adapter = self.video_provider(provider)?;
        adapter
            .start_video_generation(ProviderVideoStartRequest { request })
            .await
    }

    async fn poll_video_generation(
        &self,
        model_id: &str,
        operation_name: &str,
        poll_interval_secs: Option<u64>,
        max_wait_secs: Option<u64>,
        operation: Operation,
    ) -> AiResult<VideoGenerationResponse> {
        let resolved = model_registry::resolve_model_id(model_id, operation);
        self.poll_video_generation_with_provider(
            resolved.provider,
            resolved.model_id,
            operation_name,
            poll_interval_secs,
            max_wait_secs,
            resolved.operation,
        )
        .await
    }

    pub async fn poll_video_generation_with_provider(
        &self,
        provider: Provider,
        model_id: String,
        operation_name: &str,
        poll_interval_secs: Option<u64>,
        max_wait_secs: Option<u64>,
        operation: Operation,
    ) -> AiResult<VideoGenerationResponse> {
        capability::ensure_supports(provider, operation)?;
        let adapter = self.video_provider(provider)?;
        adapter
            .poll_video_generation(ProviderVideoPollRequest {
                model_id,
                operation_name: operation_name.to_string(),
                poll_interval_secs,
                max_wait_secs,
            })
            .await
    }

    pub async fn tool_calling(
        &self,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ToolCallingResponse> {
        let resolved = model_registry::resolve_model_id(model_id, Operation::ToolCalling);
        self.tool_calling_with_provider(resolved.provider, &resolved.model_id, payload)
            .await
    }

    pub async fn tool_calling_with_provider(
        &self,
        provider: Provider,
        model_id: &str,
        payload: Value,
    ) -> AiResult<ToolCallingResponse> {
        capability::ensure_supports(provider, Operation::ToolCalling)?;
        let adapter = self.tool_calling_provider(provider)?;
        Ok(adapter
            .tool_calling(ProviderToolCallingRequest {
                model_id: model_id.to_string(),
                payload,
            })
            .await?
            .data)
    }

    pub async fn connect_realtime_conversation(
        &self,
        provider: Provider,
        request: RealtimeConnectRequest,
    ) -> AiResult<Arc<dyn RealtimeConversationSession>> {
        capability::ensure_supports(provider, Operation::RealtimeConversation)?;
        let adapter = self.realtime_provider(provider)?;
        adapter.connect_realtime_conversation(request).await
    }

    fn text_provider(&self, provider: Provider) -> AiResult<&Arc<dyn TextProvider>> {
        self.text
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "text"))
    }

    fn image_provider(&self, provider: Provider) -> AiResult<&Arc<dyn ImageProvider>> {
        self.image
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "image"))
    }

    fn audio_provider(&self, provider: Provider) -> AiResult<&Arc<dyn AudioProvider>> {
        self.audio
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "audio"))
    }

    fn video_provider(&self, provider: Provider) -> AiResult<&Arc<dyn VideoProvider>> {
        self.video
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "video"))
    }

    fn tool_calling_provider(&self, provider: Provider) -> AiResult<&Arc<dyn ToolCallingProvider>> {
        self.tool_calling
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "tool_calling"))
    }

    fn realtime_provider(
        &self,
        provider: Provider,
    ) -> AiResult<&Arc<dyn RealtimeConversationProvider>> {
        self.realtime
            .get(&provider)
            .ok_or_else(|| missing_adapter(provider, "realtime"))
    }
}

fn missing_adapter(provider: Provider, capability: &str) -> AiFoundationError {
    AiFoundationError::UnsupportedOperation(format!(
        "Provider {:?} has no registered {} adapter",
        provider, capability
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;

    use crate::realtime::{
        RealtimeClientEvent, RealtimeConnectRequest, RealtimeConversationProvider,
        RealtimeConversationSession, RealtimeServerEvent,
    };

    struct MockTextProvider;

    #[async_trait]
    impl TextProvider for MockTextProvider {
        async fn text_to_text(
            &self,
            request: ProviderTextRequest,
        ) -> AiResult<crate::types::AiProviderOutput<TextGenerationResponse>> {
            let response = TextGenerationResponse {
                text: request.payload["prompt"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                usage: crate::types::Usage::default(),
                model: request.model_id.clone(),
                raw_response: None,
            };
            Ok(crate::types::AiProviderOutput {
                data: response,
                usage: Some(crate::types::Usage::default()),
                provider: Provider::Ark,
                model_id: request.model_id,
                raw: None,
            })
        }
    }

    struct MockRealtimeProvider;

    #[async_trait]
    impl RealtimeConversationProvider for MockRealtimeProvider {
        async fn connect_realtime_conversation(
            &self,
            request: RealtimeConnectRequest,
        ) -> AiResult<Arc<dyn RealtimeConversationSession>> {
            Ok(Arc::new(MockRealtimeSession {
                connect_id: request
                    .connect_id
                    .unwrap_or_else(|| "mock-connect".to_string()),
            }))
        }
    }

    struct MockRealtimeSession {
        connect_id: String,
    }

    #[async_trait]
    impl RealtimeConversationSession for MockRealtimeSession {
        fn provider(&self) -> Provider {
            Provider::Ark
        }

        fn connect_id(&self) -> &str {
            &self.connect_id
        }

        async fn send_client_event(&self, _event: RealtimeClientEvent) -> AiResult<()> {
            Ok(())
        }

        async fn next_event(&self) -> AiResult<Option<RealtimeServerEvent>> {
            Ok(None)
        }

        async fn close(&self) -> AiResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn explicit_provider_routing_does_not_reinfer_model_id() {
        let router =
            ProviderRouter::new().with_text_provider(Provider::Ark, Arc::new(MockTextProvider));
        let response = router
            .text_to_text_with_provider(
                Provider::Ark,
                "custom-model",
                serde_json::json!({ "prompt": "ok" }),
            )
            .await
            .unwrap();

        assert_eq!(response.model, "custom-model");
        assert_eq!(response.text, "ok");
    }

    #[tokio::test]
    async fn routes_realtime_provider() {
        let router = ProviderRouter::new()
            .with_realtime_provider(Provider::Ark, Arc::new(MockRealtimeProvider));
        let session = router
            .connect_realtime_conversation(
                Provider::Ark,
                RealtimeConnectRequest::new().with_connect_id("connect-1"),
            )
            .await
            .unwrap();

        assert_eq!(session.provider(), Provider::Ark);
        assert_eq!(session.connect_id(), "connect-1");
        session
            .send_audio("session-1".to_string(), Bytes::from_static(&[1, 2]))
            .await
            .unwrap();
    }
}
