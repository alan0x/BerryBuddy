use crate::operation::{Operation, Provider};
use crate::types::ModelRef;

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub provider: Provider,
    pub model_id: String,
    pub operation: Operation,
}

pub fn resolve_model(model: ModelRef) -> ResolvedModel {
    let provider = model
        .provider
        .unwrap_or_else(|| provider_for_model_id(&model.model_id));
    ResolvedModel {
        provider,
        model_id: model.model_id,
        operation: model.operation,
    }
}

pub fn resolve_model_id(model_id: &str, operation: Operation) -> ResolvedModel {
    resolve_model(ModelRef {
        provider: None,
        model_id: model_id.to_string(),
        operation,
    })
}

pub fn provider_for_model_id(model_id: &str) -> Provider {
    let model = model_id.to_ascii_lowercase();

    if model.contains("gemini")
        || model.contains("imagen")
        || model.contains("lyria")
        || model.contains("veo")
    {
        return Provider::Vertex;
    }

    if model.starts_with("doubao")
        || model.starts_with("ep-")
        || model.contains("deepseek")
        || model == "volc.speech.dialog"
        || model.contains("openspeech")
        || model.contains("s2s")
    {
        return Provider::Ark;
    }

    Provider::Vertex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_known_provider_families() {
        assert_eq!(provider_for_model_id("gemini-2.5-pro"), Provider::Vertex);
        assert_eq!(
            provider_for_model_id("imagen-4.0-generate-001"),
            Provider::Vertex
        );
        assert_eq!(
            provider_for_model_id("doubao-seedance-1-0-lite-i2v-250428"),
            Provider::Ark
        );
        assert_eq!(provider_for_model_id("ep-20260101-example"), Provider::Ark);
        assert_eq!(provider_for_model_id("volc.speech.dialog"), Provider::Ark);
    }

    #[test]
    fn explicit_provider_overrides_model_id_heuristic() {
        let resolved = resolve_model(ModelRef {
            provider: Some(Provider::Ark),
            model_id: "custom-model".to_string(),
            operation: Operation::TextToText,
        });

        assert_eq!(resolved.provider, Provider::Ark);
        assert_eq!(resolved.model_id, "custom-model");
    }
}
