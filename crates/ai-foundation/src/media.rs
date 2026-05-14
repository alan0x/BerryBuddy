use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaInput {
    Url { url: String },
    Base64 { mime_type: String, data: String },
    Bytes { mime_type: String, data: Bytes },
    GcsUri { uri: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaRef {
    Url { url: String },
    Base64 { mime_type: String, data: String },
    GcsUri { uri: String },
}
