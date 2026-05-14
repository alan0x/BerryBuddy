use crate::media::MediaInput;
use crate::operation::Operation;
use crate::types::ModelRef;

#[derive(Debug, Clone, Default)]
pub struct GenerationOptions {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<i32>,
    pub response_mime_type: Option<String>,
    pub response_schema: Option<serde_json::Value>,
    pub response_modalities: Option<Vec<String>>,
    pub safety_settings: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct TextToTextRequest {
    pub model: ModelRef,
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub options: GenerationOptions,
}

#[derive(Debug, Clone)]
pub struct TextChatRequest {
    pub model: ModelRef,
    pub system_prompt: Option<String>,
    pub contents: Vec<serde_json::Value>,
    pub options: GenerationOptions,
}

#[derive(Debug, Clone)]
pub struct TextToImageRequest {
    pub model: ModelRef,
    pub prompt: String,
    pub aspect_ratio: Option<String>,
    pub sample_count: Option<i32>,
    pub negative_prompt: Option<String>,
    pub person_generation: Option<String>,
    pub safety_setting: Option<String>,
    pub add_watermark: Option<bool>,
    pub include_rai_reason: Option<bool>,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TextToAudioRequest {
    pub model: ModelRef,
    pub prompt: String,
    pub seed: Option<i32>,
    pub sample_count: Option<i32>,
    pub voice: Option<String>,
    pub response_format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpeechSpeaker {
    pub alias: String,
    pub voice_name: String,
}

#[derive(Debug, Clone)]
pub struct TextToSpeechRequest {
    pub model: ModelRef,
    pub text: String,
    pub language_code: String,
    pub voice_name: Option<String>,
    pub speakers: Vec<SpeechSpeaker>,
    pub style_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoGenerationMode {
    TextToVideo,
    FirstFrameToVideo,
    FirstLastFrameToVideo,
    ImageReferenceToVideo,
    MultimodalReferenceToVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoReferenceKind {
    ProductReference,
    CharacterReference,
    EnvironmentReference,
    StyleReference,
    FirstFrame,
    LastFrame,
    VideoReference,
    AudioReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoReferenceRequirement {
    Required,
    Optional,
}

#[derive(Debug, Clone)]
pub struct VideoReference {
    pub kind: VideoReferenceKind,
    pub media: MediaInput,
    pub requirement: VideoReferenceRequirement,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VideoOutputSpec {
    pub ratio: Option<String>,
    pub duration_seconds: Option<i64>,
    pub frames: Option<i64>,
    pub resolution: Option<String>,
    pub seed: Option<i64>,
    pub camera_fixed: Option<bool>,
    pub watermark: Option<bool>,
    pub generate_audio: Option<bool>,
    pub return_last_frame: Option<bool>,
    pub service_tier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VideoGenerationRequest {
    pub model: ModelRef,
    pub mode: VideoGenerationMode,
    pub prompt: String,
    pub output: VideoOutputSpec,
    pub references: Vec<VideoReference>,
    pub poll_interval_secs: Option<u64>,
    pub max_wait_secs: Option<u64>,
}

impl VideoGenerationRequest {
    pub fn new(
        model_id: impl Into<String>,
        mode: VideoGenerationMode,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            model: ModelRef {
                provider: None,
                model_id: model_id.into(),
                operation: Operation::VideoGeneration,
            },
            mode,
            prompt: prompt.into(),
            output: VideoOutputSpec::default(),
            references: Vec::new(),
            poll_interval_secs: None,
            max_wait_secs: None,
        }
    }

    pub fn with_output(mut self, output: VideoOutputSpec) -> Self {
        self.output = output;
        self
    }

    pub fn with_references(mut self, references: Vec<VideoReference>) -> Self {
        self.references = references;
        self
    }

    pub fn with_polling(
        mut self,
        poll_interval_secs: Option<u64>,
        max_wait_secs: Option<u64>,
    ) -> Self {
        self.poll_interval_secs = poll_interval_secs;
        self.max_wait_secs = max_wait_secs;
        self
    }
}

impl TextToTextRequest {
    pub fn new(model_id: impl Into<String>, user_prompt: impl Into<String>) -> Self {
        Self {
            model: ModelRef {
                provider: None,
                model_id: model_id.into(),
                operation: Operation::TextToText,
            },
            system_prompt: None,
            user_prompt: user_prompt.into(),
            options: GenerationOptions::default(),
        }
    }
}

impl TextChatRequest {
    pub fn new(model_id: impl Into<String>, contents: Vec<serde_json::Value>) -> Self {
        Self {
            model: ModelRef {
                provider: None,
                model_id: model_id.into(),
                operation: Operation::TextToText,
            },
            system_prompt: None,
            contents,
            options: GenerationOptions::default(),
        }
    }
}
