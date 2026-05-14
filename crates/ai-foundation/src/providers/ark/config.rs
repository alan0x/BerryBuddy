#[derive(Debug, Clone)]
pub struct ArkProviderConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout_secs: u64,
}

impl ArkProviderConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            timeout_secs: 660,
        }
    }
}
