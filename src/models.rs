use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Status Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Starting,
    Idle,
    Running,
    Paused,
    Syncing,
    Error,
    Stopping,
}

impl ServiceStatus {
    pub const ALL: &[ServiceStatus] = &[
        Self::Starting, Self::Idle, Self::Running, Self::Paused,
        Self::Syncing, Self::Error, Self::Stopping,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Syncing => "syncing",
            Self::Error => "error",
            Self::Stopping => "stopping",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Pending,
    Acknowledged,
    Completed,
    Failed,
    Expired,
}

impl CommandStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Acknowledged => "acknowledged",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }
}

impl std::fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventType {
    Scheduled,
    Manual,
    Command,
    Triggered,
    FullSync,
}

impl SyncEventType {
    pub const ALL: &[SyncEventType] = &[
        Self::Scheduled, Self::Manual, Self::Command, Self::Triggered, Self::FullSync,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::Command => "command",
            Self::Triggered => "triggered",
            Self::FullSync => "full_sync",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventStatus {
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
}

impl SyncEventStatus {
    pub const ALL: &[SyncEventStatus] = &[
        Self::Running, Self::Completed, Self::Partial, Self::Failed, Self::Cancelled,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Partial | Self::Failed | Self::Cancelled)
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Completed | Self::Partial)
    }
}

// ============================================================================
// Server Configuration
// ============================================================================

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

// ============================================================================
// Enrollment
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EnrollRequest {
    pub client_id: String,
    pub client_secret: String,
    pub instance_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EnrollResponse {
    pub service_id: Uuid,
    pub session_token: String,
}

// ============================================================================
// Heartbeat
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HeartbeatRequest {
    pub service_id: Uuid,
    pub status: String,
    pub current_operation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HeartbeatResponse {
    pub session_token: String,
    pub pending_commands: Vec<PendingCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PendingCommand {
    pub id: Uuid,
    pub command: String,
    #[cfg_attr(feature = "server", schema(value_type = Object))]
    pub payload: Option<serde_json::Value>,
}

// ============================================================================
// Command Updates
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CommandUpdateRequest {
    pub status: String,
    #[cfg_attr(feature = "server", schema(value_type = Object))]
    pub result: Option<serde_json::Value>,
}

// ============================================================================
// Sync Result
// ============================================================================

#[derive(Debug, Default, Serialize)]
pub struct SyncResult {
    pub readings_synced: u64,
    pub status_events_synced: u64,
    pub full_sync: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
}

#[derive(Debug)]
pub enum SyncTrigger {
    Scheduled,
    Command { id: Uuid, full: bool },
}

// ============================================================================
// Runner Config
// ============================================================================

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub api_base_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub instance_id: String,
    pub heartbeat_interval_secs: u64,
    pub sync_interval_secs: u64,
    pub enrollment_retry_secs: u64,
}

impl RunnerConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            api_base_url: require_env("API_BASE_URL")?,
            client_id: require_env("SERVICE_CLIENT_ID")?,
            client_secret: require_env("SERVICE_CLIENT_SECRET")?,
            instance_id: std::env::var("INSTANCE_ID").unwrap_or_else(|_| "default".to_string()),
            heartbeat_interval_secs: env_u64("HEARTBEAT_INTERVAL_SECONDS", 30),
            sync_interval_secs: env_u64("SYNC_INTERVAL_SECONDS", 300),
            enrollment_retry_secs: env_u64("ENROLLMENT_RETRY_SECONDS", 10),
        })
    }
}

fn require_env(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("Missing required env var: {key}"))
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ============================================================================
// River Data API types (shared across sync services)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStream {
    pub id: Uuid,
    pub source_system: String,
    pub source_key: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub metadata: serde_json::Value,
    pub site_parameter_id: Option<Uuid>,
    /// Stream-level default for readings.measurement_type ('continuous' | 'spot' | 'derived').
    /// None defers to the API's sensor-frequency resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    pub is_active: bool,
    pub last_data_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RegisterStreamRequest {
    pub source_system: String,
    pub source_key: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub metadata: serde_json::Value,
    /// Stream-level classification declared at discovery. None never clears an operator-set value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestReading {
    pub time: chrono::DateTime<chrono::Utc>,
    pub raw_value: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub replicate_index: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibration_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<Uuid>,
    /// Per-reading override ('continuous' | 'spot' | 'derived'). None resolves server-side from
    /// the stream default, then the owning sensor's data_frequency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
}

fn is_zero(v: &i16) -> bool {
    *v == 0
}

#[derive(Debug, Serialize)]
pub struct IngestStatusEvent {
    pub time: chrono::DateTime<chrono::Utc>,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enroll_request_serialization() {
        let req = EnrollRequest {
            client_id: "svc_abc".to_string(),
            client_secret: "secret123".to_string(),
            instance_id: "service-01".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["client_id"], "svc_abc");
        assert_eq!(json["instance_id"], "service-01");
    }

    #[test]
    fn test_enroll_response_deserialization() {
        let json = serde_json::json!({
            "service_id": "550e8400-e29b-41d4-a716-446655440000",
            "session_token": "tok-abc"
        });
        let resp: EnrollResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.session_token, "tok-abc");
    }

    #[test]
    fn test_heartbeat_response_with_commands() {
        let json = serde_json::json!({
            "session_token": "new-tok",
            "pending_commands": [
                {
                    "id": "550e8400-e29b-41d4-a716-446655440000",
                    "command": "trigger_sync",
                    "payload": null
                }
            ]
        });
        let resp: HeartbeatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.pending_commands.len(), 1);
        assert_eq!(resp.pending_commands[0].command, "trigger_sync");
    }

    #[test]
    fn test_sync_result_default() {
        let r = SyncResult::default();
        assert_eq!(r.readings_synced, 0);
        assert!(!r.full_sync);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_sync_result_serialization_skips_empty() {
        let r = SyncResult {
            readings_synced: 100,
            ..Default::default()
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["readings_synced"], 100);
        assert!(json.get("errors").is_none());
    }

    #[test]
    fn test_ingest_reading_serialization() {
        let r = IngestReading {
            time: chrono::Utc::now(),
            raw_value: 42.5,
            replicate_index: 0,
            sensor_id: None,
            calibration_id: None,
            deployment_id: None,
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["raw_value"], 42.5);
        assert!(json.get("replicate_index").is_none());
        assert!(json.get("sensor_id").is_none());
    }

    #[test]
    fn test_register_stream_request() {
        let req = RegisterStreamRequest {
            source_system: "test_system".to_string(),
            source_key: "source_1".to_string(),
            source_name: Some("stream_a".to_string()),
            source_path: None,
            metadata: serde_json::json!({"device": "dev_001"}),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["source_system"], "test_system");
        assert_eq!(json["metadata"]["device"], "dev_001");
    }

    #[test]
    fn test_data_stream_deserialization() {
        let json = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "source_system": "test_system",
            "source_key": "source_1",
            "source_name": "stream_a",
            "source_path": null,
            "metadata": {},
            "site_parameter_id": null,
            "is_active": true,
            "last_data_time": null
        });
        let stream: DataStream = serde_json::from_value(json).unwrap();
        assert_eq!(stream.source_system, "test_system");
        assert!(stream.is_active);
        assert!(stream.site_parameter_id.is_none());
    }
}
