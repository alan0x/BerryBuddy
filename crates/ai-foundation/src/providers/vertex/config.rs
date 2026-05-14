#[derive(Debug, Clone)]
pub struct VertexProviderConfig {
    pub project_id: String,
    pub location: String,
    pub auth: VertexAuth,
    pub timeout_secs: u64,
    pub gcs: Option<VertexGcsStagingConfig>,
}

#[derive(Debug, Clone)]
pub enum VertexAuth {
    AccessToken(String),
    ServiceAccountJson(String),
}

#[derive(Debug, Clone)]
pub struct VertexGcsStagingConfig {
    pub bucket: String,
    pub credentials: GcsCredentials,
    pub object_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub enum GcsCredentials {
    AccessToken(String),
    ServiceAccountJson(String),
}
