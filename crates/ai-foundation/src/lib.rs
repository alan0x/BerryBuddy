#![forbid(unsafe_code)]

pub mod error;
pub mod generic;
pub mod media;
pub mod operation;
pub mod providers;
pub mod realtime;
pub mod types;

pub use error::{AiFoundationError, AiResult};
pub use media::{MediaInput, MediaRef};
pub use operation::{Operation, Provider};
pub use realtime::{
    RealtimeAsrResult, RealtimeClientEvent, RealtimeConnectRequest, RealtimeConversationProvider,
    RealtimeConversationSession, RealtimeEventId, RealtimeInputMode, RealtimeServerEvent,
    RealtimeStartSessionRequest, RealtimeUsage,
};
pub use types::{
    AiAssetRef, AiProviderOutput, AudioGenerationResponse, GeneratedAsset, ImageGenerationResponse,
    ModelRef, ModelSpec, SpeechGenerationResponse, TextGenerationResponse, ToolCallingResponse,
    Usage, VideoGenerationResponse,
};
