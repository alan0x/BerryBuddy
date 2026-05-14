use gcp_auth::TokenProvider;
use mime_guess::MimeGuess;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::error::{AiFoundationError, AiResult};
use crate::providers::vertex::config::{GcsCredentials, VertexGcsStagingConfig};

#[derive(Debug, Clone)]
pub struct VertexGcsStaging {
    config: VertexGcsStagingConfig,
}

impl VertexGcsStaging {
    pub fn new(config: VertexGcsStagingConfig) -> Self {
        Self { config }
    }

    pub async fn upload_file(&self, file_path: &str, object_name: &str) -> AiResult<String> {
        let bytes = tokio::fs::read(file_path).await.map_err(|error| {
            AiFoundationError::Storage(format!("failed to read {}: {}", file_path, error))
        })?;
        let content_type = MimeGuess::from_path(file_path)
            .first_or_octet_stream()
            .to_string();
        self.upload_bytes(bytes, object_name, &content_type).await
    }

    pub async fn upload_bytes(
        &self,
        bytes: Vec<u8>,
        object_name: &str,
        content_type: &str,
    ) -> AiResult<String> {
        let object_name = self.object_name(object_name);
        let client = gcs_client(&self.config.credentials).await?;
        let base = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o",
            self.config.bucket
        );
        let mut url = reqwest::Url::parse(&base)
            .map_err(|error| AiFoundationError::Storage(error.to_string()))?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("uploadType", "media");
            qp.append_pair("name", &object_name);
        }

        let response = client
            .post(url)
            .header(CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|error| AiFoundationError::Network(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AiFoundationError::Storage(format!(
                "GCS upload failed: status={} body={}",
                status, body
            )));
        }

        let metadata: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));
        let name = metadata
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(&object_name);
        Ok(format!("gs://{}/{}", self.config.bucket, name))
    }

    fn object_name(&self, object_name: &str) -> String {
        match self.config.object_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => format!(
                "{}/{}",
                prefix.trim_matches('/'),
                object_name.trim_start_matches('/')
            ),
            _ => object_name.to_string(),
        }
    }
}

async fn gcs_client(credentials: &GcsCredentials) -> AiResult<reqwest::Client> {
    match credentials {
        GcsCredentials::AccessToken(token) => client_with_token(token),
        GcsCredentials::ServiceAccountJson(json) => {
            let token = service_account_token(json).await?;
            client_with_token(&token)
        }
    }
}

fn client_with_token(token: &str) -> AiResult<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        format!("Bearer {}", token).parse().map_err(|error| {
            AiFoundationError::Storage(format!("invalid authorization header: {}", error))
        })?,
    );
    reqwest::Client::builder()
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

    #[test]
    fn applies_object_prefix_to_staged_object_name() {
        let staging = VertexGcsStaging::new(VertexGcsStagingConfig {
            bucket: "bucket".to_string(),
            credentials: GcsCredentials::AccessToken("token".to_string()),
            object_prefix: Some("/prefix/".to_string()),
        });

        assert_eq!(
            staging.object_name("/temp/file.png"),
            "prefix/temp/file.png"
        );
    }

    #[test]
    fn leaves_object_name_unmodified_without_prefix() {
        let staging = VertexGcsStaging::new(VertexGcsStagingConfig {
            bucket: "bucket".to_string(),
            credentials: GcsCredentials::AccessToken("token".to_string()),
            object_prefix: None,
        });

        assert_eq!(staging.object_name("temp/file.png"), "temp/file.png");
    }
}
