use crate::operation::Provider;

pub type AiResult<T> = Result<T, AiFoundationError>;

#[derive(Debug, thiserror::Error)]
pub enum AiFoundationError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("provider {provider:?} error: {message}")]
    ProviderError { provider: Provider, message: String },

    #[error("network error: {0}")]
    Network(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("storage error: {0}")]
    Storage(String),
}
