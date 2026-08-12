use crate::env;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub api_base_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub instance_id: String,
    pub heartbeat_interval_secs: u64,
    pub sync_interval_secs: u64,
    pub enrollment_retry_secs: u64,
    pub retry_max: u32,
    pub retry_delay_secs: u64,
}

impl RunnerConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            api_base_url: env::require("API_BASE_URL")?,
            client_id: env::require("SERVICE_CLIENT_ID")?,
            client_secret: env::require("SERVICE_CLIENT_SECRET")?,
            instance_id: env::string_or("INSTANCE_ID", "default"),
            heartbeat_interval_secs: env::parse_or("HEARTBEAT_INTERVAL_SECONDS", 30),
            sync_interval_secs: env::parse_or("SYNC_INTERVAL_SECONDS", 300),
            enrollment_retry_secs: env::parse_or("ENROLLMENT_RETRY_SECONDS", 10),
            retry_max: env::parse_or("RETRY_MAX", 3),
            retry_delay_secs: env::parse_or("RETRY_DELAY_SECONDS", 60),
        })
    }
}
