use crate::env;

#[derive(Debug, Clone)]
pub struct SyncServerConfig {
    pub session_token_ttl_secs: u64,
    pub token_cache_capacity: u64,
    pub token_cache_ttl_secs: u64,
    pub command_expiry_secs: u64,
    pub health_healthy_secs: i64,
    pub health_warning_secs: i64,
    pub client_id_prefix: String,
}

impl Default for SyncServerConfig {
    fn default() -> Self {
        Self {
            session_token_ttl_secs: 900,
            token_cache_capacity: 100,
            token_cache_ttl_secs: 780,
            command_expiry_secs: 300,
            health_healthy_secs: 90,
            health_warning_secs: 300,
            client_id_prefix: "svc_".to_string(),
        }
    }
}

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
