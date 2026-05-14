use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Vertex,
    Ark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    TextToText,
    MultimodalToText,
    TextToImage,
    TextToVideo,
    VideoGeneration,
    TextToAudio,
    ImageToImage,
    ImageToVideo,
    ImageToText,
    VideoToVideo,
    VideoToImage,
    VideoToText,
    AudioToText,
    RealtimeConversation,
    ToolCalling,
}
