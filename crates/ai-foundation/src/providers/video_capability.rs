use crate::error::{AiFoundationError, AiResult};
use crate::generic::types::{VideoGenerationMode, VideoGenerationRequest, VideoReferenceKind};
use crate::operation::Provider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationRange {
    pub min_seconds: i64,
    pub max_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoModelCapability {
    pub provider: Provider,
    pub model_id: &'static str,
    pub modes: &'static [VideoGenerationMode],
    pub max_image_references: usize,
    pub max_video_references: usize,
    pub max_audio_references: usize,
    pub supports_generate_audio: bool,
    pub supports_return_last_frame: bool,
    pub duration_range: DurationRange,
    pub ratios: &'static [&'static str],
    pub resolutions: &'static [&'static str],
    pub service_tiers: &'static [&'static str],
}

const RATIOS_WITH_ADAPTIVE: &[&str] = &["21:9", "16:9", "4:3", "1:1", "3:4", "9:16", "adaptive"];
const FIXED_RATIOS: &[&str] = &["21:9", "16:9", "4:3", "1:1", "3:4", "9:16"];
const VERTEX_31_RATIOS: &[&str] = &["16:9", "9:16"];

const RES_480_720: &[&str] = &["480p", "720p"];
const RES_480_720_1080: &[&str] = &["480p", "720p", "1080p"];
const VERTEX_31_RESOLUTIONS: &[&str] = &["720p", "1080p"];

const DEFAULT_SERVICE_TIERS: &[&str] = &["default"];
const ARK_SERVICE_TIERS: &[&str] = &["default", "flex"];

const TEXT_ONLY: &[VideoGenerationMode] = &[VideoGenerationMode::TextToVideo];
const FIRST_FRAME: &[VideoGenerationMode] = &[
    VideoGenerationMode::TextToVideo,
    VideoGenerationMode::FirstFrameToVideo,
];
const FIRST_LAST_FRAME: &[VideoGenerationMode] = &[
    VideoGenerationMode::TextToVideo,
    VideoGenerationMode::FirstFrameToVideo,
    VideoGenerationMode::FirstLastFrameToVideo,
];
const IMAGE_REFERENCE: &[VideoGenerationMode] = &[
    VideoGenerationMode::TextToVideo,
    VideoGenerationMode::ImageReferenceToVideo,
];
const VERTEX_31_REFERENCE: &[VideoGenerationMode] = &[
    VideoGenerationMode::TextToVideo,
    VideoGenerationMode::FirstFrameToVideo,
    VideoGenerationMode::FirstLastFrameToVideo,
    VideoGenerationMode::ImageReferenceToVideo,
];
const MULTIMODAL_REFERENCE: &[VideoGenerationMode] = &[
    VideoGenerationMode::TextToVideo,
    VideoGenerationMode::FirstFrameToVideo,
    VideoGenerationMode::FirstLastFrameToVideo,
    VideoGenerationMode::ImageReferenceToVideo,
    VideoGenerationMode::MultimodalReferenceToVideo,
];

const CAPABILITIES: &[VideoModelCapability] = &[
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-2-0-260128",
        modes: MULTIMODAL_REFERENCE,
        max_image_references: 9,
        max_video_references: 3,
        max_audio_references: 3,
        supports_generate_audio: true,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 4,
            max_seconds: 15,
        },
        ratios: RATIOS_WITH_ADAPTIVE,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-2-0-fast-260128",
        modes: MULTIMODAL_REFERENCE,
        max_image_references: 9,
        max_video_references: 3,
        max_audio_references: 3,
        supports_generate_audio: true,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 4,
            max_seconds: 15,
        },
        ratios: RATIOS_WITH_ADAPTIVE,
        resolutions: RES_480_720,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-1-5-pro-251215",
        modes: FIRST_LAST_FRAME,
        max_image_references: 2,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: true,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 4,
            max_seconds: 12,
        },
        ratios: RATIOS_WITH_ADAPTIVE,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-1-0-pro-250528",
        modes: FIRST_LAST_FRAME,
        max_image_references: 2,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 2,
            max_seconds: 12,
        },
        ratios: FIXED_RATIOS,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-1-0-pro-fast-251015",
        modes: FIRST_FRAME,
        max_image_references: 1,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 2,
            max_seconds: 12,
        },
        ratios: FIXED_RATIOS,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-1-0-lite-i2v-250428",
        modes: IMAGE_REFERENCE,
        max_image_references: 4,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 2,
            max_seconds: 12,
        },
        ratios: FIXED_RATIOS,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Ark,
        model_id: "doubao-seedance-1-0-lite-t2v-250428",
        modes: TEXT_ONLY,
        max_image_references: 0,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: true,
        duration_range: DurationRange {
            min_seconds: 2,
            max_seconds: 12,
        },
        ratios: FIXED_RATIOS,
        resolutions: RES_480_720_1080,
        service_tiers: ARK_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Vertex,
        model_id: "veo-3.1-generate-001",
        modes: VERTEX_31_REFERENCE,
        max_image_references: 3,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: false,
        duration_range: DurationRange {
            min_seconds: 4,
            max_seconds: 8,
        },
        ratios: VERTEX_31_RATIOS,
        resolutions: VERTEX_31_RESOLUTIONS,
        service_tiers: DEFAULT_SERVICE_TIERS,
    },
    VideoModelCapability {
        provider: Provider::Vertex,
        model_id: "veo-3.1-fast-generate-001",
        modes: FIRST_LAST_FRAME,
        max_image_references: 2,
        max_video_references: 0,
        max_audio_references: 0,
        supports_generate_audio: false,
        supports_return_last_frame: false,
        duration_range: DurationRange {
            min_seconds: 4,
            max_seconds: 8,
        },
        ratios: VERTEX_31_RATIOS,
        resolutions: VERTEX_31_RESOLUTIONS,
        service_tiers: DEFAULT_SERVICE_TIERS,
    },
];

pub fn video_capabilities() -> &'static [VideoModelCapability] {
    CAPABILITIES
}

pub fn video_capability(
    provider: Provider,
    model_id: &str,
) -> Option<&'static VideoModelCapability> {
    CAPABILITIES
        .iter()
        .find(|capability| capability.provider == provider && capability.model_id == model_id)
}

pub fn ensure_video_request_supported(
    provider: Provider,
    request: &VideoGenerationRequest,
) -> AiResult<&'static VideoModelCapability> {
    let capability = video_capability(provider, &request.model.model_id).ok_or_else(|| {
        AiFoundationError::UnsupportedOperation(format!(
            "Provider {:?} has no video capability declaration for model {}",
            provider, request.model.model_id
        ))
    })?;

    if !capability.modes.contains(&request.mode) {
        return Err(AiFoundationError::UnsupportedOperation(format!(
            "Model {} does not support video mode {:?}",
            request.model.model_id, request.mode
        )));
    }

    validate_reference_shape(capability, request)?;
    validate_output_spec(capability, request)?;

    Ok(capability)
}

fn validate_reference_shape(
    capability: &VideoModelCapability,
    request: &VideoGenerationRequest,
) -> AiResult<()> {
    let image_count = request
        .references
        .iter()
        .filter(|reference| is_image_reference(reference.kind))
        .count();
    let video_count = request
        .references
        .iter()
        .filter(|reference| reference.kind == VideoReferenceKind::VideoReference)
        .count();
    let audio_count = request
        .references
        .iter()
        .filter(|reference| reference.kind == VideoReferenceKind::AudioReference)
        .count();

    if image_count > capability.max_image_references {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} supports at most {} image references, got {}.",
            capability.model_id, capability.max_image_references, image_count
        )));
    }

    if video_count > capability.max_video_references {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} supports at most {} video references, got {}.",
            capability.model_id, capability.max_video_references, video_count
        )));
    }

    if audio_count > capability.max_audio_references {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} supports at most {} audio references, got {}.",
            capability.model_id, capability.max_audio_references, audio_count
        )));
    }

    if capability.provider == Provider::Vertex
        && capability.model_id != "veo-2.0-generate-exp"
        && has_reference_kind(request, VideoReferenceKind::StyleReference)
    {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support Vertex style reference images.",
            capability.model_id
        )));
    }

    match request.mode {
        VideoGenerationMode::TextToVideo => {
            if !request.references.is_empty() {
                return Err(AiFoundationError::InvalidRequest(
                    "Text-to-video request must not include references.".to_string(),
                ));
            }
        }
        VideoGenerationMode::FirstFrameToVideo => {
            if image_count != 1 || !has_reference_kind(request, VideoReferenceKind::FirstFrame) {
                return Err(AiFoundationError::InvalidRequest(
                    "First-frame video request requires exactly one first_frame reference."
                        .to_string(),
                ));
            }
        }
        VideoGenerationMode::FirstLastFrameToVideo => {
            if image_count != 2
                || !has_reference_kind(request, VideoReferenceKind::FirstFrame)
                || !has_reference_kind(request, VideoReferenceKind::LastFrame)
            {
                return Err(AiFoundationError::InvalidRequest(
                    "First-last-frame video request requires first_frame and last_frame references.".to_string(),
                ));
            }
        }
        VideoGenerationMode::ImageReferenceToVideo => {
            if image_count == 0 {
                return Err(AiFoundationError::InvalidRequest(
                    "Image-reference video request requires at least one image reference."
                        .to_string(),
                ));
            }
        }
        VideoGenerationMode::MultimodalReferenceToVideo => {
            if request.references.is_empty() {
                return Err(AiFoundationError::InvalidRequest(
                    "Multimodal-reference video request requires at least one reference."
                        .to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_output_spec(
    capability: &VideoModelCapability,
    request: &VideoGenerationRequest,
) -> AiResult<()> {
    if let Some(duration) = request.output.duration_seconds
        && (duration < capability.duration_range.min_seconds
            || duration > capability.duration_range.max_seconds)
    {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} duration must be in [{}..={}], got {}.",
            capability.model_id,
            capability.duration_range.min_seconds,
            capability.duration_range.max_seconds,
            duration
        )));
    }

    if let Some(ratio) = request.output.ratio.as_deref()
        && !capability.ratios.contains(&ratio)
    {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support ratio {}.",
            capability.model_id, ratio
        )));
    }

    if let Some(resolution) = request.output.resolution.as_deref()
        && !capability.resolutions.contains(&resolution)
    {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support resolution {}.",
            capability.model_id, resolution
        )));
    }

    if request.output.generate_audio == Some(true) && !capability.supports_generate_audio {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support generate_audio.",
            capability.model_id
        )));
    }

    if request.output.return_last_frame == Some(true) && !capability.supports_return_last_frame {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support return_last_frame.",
            capability.model_id
        )));
    }

    if let Some(service_tier) = request.output.service_tier.as_deref()
        && !capability.service_tiers.contains(&service_tier)
    {
        return Err(AiFoundationError::InvalidRequest(format!(
            "Model {} does not support service_tier {}.",
            capability.model_id, service_tier
        )));
    }

    Ok(())
}

fn has_reference_kind(request: &VideoGenerationRequest, kind: VideoReferenceKind) -> bool {
    request
        .references
        .iter()
        .any(|reference| reference.kind == kind)
}

fn is_image_reference(kind: VideoReferenceKind) -> bool {
    matches!(
        kind,
        VideoReferenceKind::ProductReference
            | VideoReferenceKind::CharacterReference
            | VideoReferenceKind::EnvironmentReference
            | VideoReferenceKind::StyleReference
            | VideoReferenceKind::FirstFrame
            | VideoReferenceKind::LastFrame
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::types::{VideoOutputSpec, VideoReference, VideoReferenceRequirement};
    use crate::media::MediaInput;

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
    fn declares_seedance_2_multimodal_reference() {
        let request = VideoGenerationRequest::new(
            "doubao-seedance-2-0-260128",
            VideoGenerationMode::MultimodalReferenceToVideo,
            "make an ad shot",
        )
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

        assert!(ensure_video_request_supported(Provider::Ark, &request).is_ok());
    }

    #[test]
    fn rejects_vertex_multimodal_reference() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-generate-001",
            VideoGenerationMode::MultimodalReferenceToVideo,
            "make an ad shot",
        )
        .with_references(vec![image(VideoReferenceKind::ProductReference)]);

        assert!(ensure_video_request_supported(Provider::Vertex, &request).is_err());
    }

    #[test]
    fn declares_vertex_31_fast_text_to_video() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-fast-generate-001",
            VideoGenerationMode::TextToVideo,
            "make an ad shot",
        )
        .with_output(VideoOutputSpec {
            duration_seconds: Some(8),
            ratio: Some("16:9".to_string()),
            resolution: Some("720p".to_string()),
            ..Default::default()
        });

        assert!(ensure_video_request_supported(Provider::Vertex, &request).is_ok());
    }

    #[test]
    fn declares_vertex_31_asset_reference_video() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-generate-001",
            VideoGenerationMode::ImageReferenceToVideo,
            "make an ad shot",
        )
        .with_references(vec![image(VideoReferenceKind::ProductReference)]);

        assert!(ensure_video_request_supported(Provider::Vertex, &request).is_ok());
    }

    #[test]
    fn rejects_vertex_31_fast_reference_image_video() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-fast-generate-001",
            VideoGenerationMode::ImageReferenceToVideo,
            "make an ad shot",
        )
        .with_references(vec![image(VideoReferenceKind::ProductReference)]);

        assert!(ensure_video_request_supported(Provider::Vertex, &request).is_err());
    }

    #[test]
    fn rejects_vertex_31_style_reference_video() {
        let request = VideoGenerationRequest::new(
            "veo-3.1-generate-001",
            VideoGenerationMode::ImageReferenceToVideo,
            "make a style-driven shot",
        )
        .with_references(vec![image(VideoReferenceKind::StyleReference)]);

        assert!(ensure_video_request_supported(Provider::Vertex, &request).is_err());
    }

    #[test]
    fn validates_seedance_2_duration_range() {
        let request = VideoGenerationRequest::new(
            "doubao-seedance-2-0-260128",
            VideoGenerationMode::TextToVideo,
            "make an ad shot",
        )
        .with_output(VideoOutputSpec {
            duration_seconds: Some(3),
            ..Default::default()
        });

        assert!(ensure_video_request_supported(Provider::Ark, &request).is_err());
    }
}
