use crate::error::{AiFoundationError, AiResult};
use crate::operation::{Operation, Provider};

#[derive(Debug, Clone, Copy)]
pub struct ProviderCapability {
    pub provider: Provider,
    pub operations: &'static [Operation],
}

const VERTEX_OPERATIONS: &[Operation] = &[
    Operation::TextToText,
    Operation::MultimodalToText,
    Operation::TextToImage,
    Operation::TextToVideo,
    Operation::VideoGeneration,
    Operation::TextToAudio,
    Operation::ImageToImage,
    Operation::ImageToVideo,
    Operation::ImageToText,
    Operation::VideoToText,
    Operation::ToolCalling,
];

const ARK_OPERATIONS: &[Operation] = &[
    Operation::TextToText,
    Operation::MultimodalToText,
    Operation::TextToImage,
    Operation::TextToVideo,
    Operation::VideoGeneration,
    Operation::TextToAudio,
    Operation::ImageToText,
    Operation::ImageToImage,
    Operation::ImageToVideo,
    Operation::VideoToText,
    Operation::RealtimeConversation,
    Operation::ToolCalling,
];

const CAPABILITIES: &[ProviderCapability] = &[
    ProviderCapability {
        provider: Provider::Vertex,
        operations: VERTEX_OPERATIONS,
    },
    ProviderCapability {
        provider: Provider::Ark,
        operations: ARK_OPERATIONS,
    },
];

pub fn capabilities() -> &'static [ProviderCapability] {
    CAPABILITIES
}

pub fn supports(provider: Provider, operation: Operation) -> bool {
    CAPABILITIES
        .iter()
        .find(|capability| capability.provider == provider)
        .map(|capability| capability.operations.contains(&operation))
        .unwrap_or(false)
}

pub fn ensure_supports(provider: Provider, operation: Operation) -> AiResult<()> {
    if supports(provider, operation) {
        return Ok(());
    }

    Err(AiFoundationError::UnsupportedOperation(format!(
        "Provider {:?} does not declare support for {:?}",
        provider, operation
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_vertex_tool_calling() {
        assert!(supports(Provider::Vertex, Operation::ToolCalling));
    }

    #[test]
    fn declares_ark_tool_calling() {
        assert!(supports(Provider::Ark, Operation::ToolCalling));
        assert!(ensure_supports(Provider::Ark, Operation::ToolCalling).is_ok());
    }

    #[test]
    fn declares_ark_realtime_conversation() {
        assert!(supports(Provider::Ark, Operation::RealtimeConversation));
        assert!(!supports(Provider::Vertex, Operation::RealtimeConversation));
    }
}
