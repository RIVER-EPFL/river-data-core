use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::status::{SyncEventStatus, SyncEventType};

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollRequest {
    pub client_id: String,
    pub client_secret: String,
    pub instance_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollResponse {
    pub service_id: Uuid,
    pub session_token: String,
    /// Operator-desired pause state, persisted server-side; honored before the
    /// initial sync so a restart cannot undo a pause.
    #[serde(default)]
    pub paused: bool,
    /// Operator-set scheduled sync cadence, persisted server-side. None leaves the
    /// service on its own `SYNC_INTERVAL_SECONDS`.
    #[serde(default)]
    pub sync_interval_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HeartbeatRequest {
    pub service_id: Uuid,
    pub status: String,
    pub current_operation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HeartbeatResponse {
    pub session_token: String,
    pub pending_commands: Vec<PendingCommand>,
    /// Operator-desired pause state, persisted server-side.
    #[serde(default)]
    pub paused: bool,
    /// Operator-set scheduled sync cadence, persisted server-side. A change here is
    /// adopted by the running service on the next heartbeat.
    #[serde(default)]
    pub sync_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PendingCommand {
    pub id: Uuid,
    pub command: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommandUpdateRequest {
    pub status: String,
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct SyncEventCreate {
    pub service_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<Uuid>,
    pub event_type: SyncEventType,
    pub status: SyncEventStatus,
}

#[derive(Debug, Deserialize)]
pub struct SyncEventRef {
    pub id: Uuid,
}

#[derive(Debug, Default, Serialize)]
pub struct SyncEventUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SyncEventStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readings_synced: Option<u64>,
    /// Readings the API refused admission. Carried on the event rather than left to the process
    /// log, so a stream losing rows every cycle leaves a queryable trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readings_skipped: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_events_synced: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Outcome of one sync cycle. The runner fills `full_sync` and `duration_ms`;
/// a `SyncService` only reports counts, errors and log lines.
#[derive(Debug, Default, Serialize)]
pub struct SyncResult {
    pub readings_synced: u64,
    /// Readings the API refused admission and dropped. Additive: a reader that
    /// predates the field must still parse the rest.
    #[serde(default)]
    pub readings_skipped: u64,
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

#[cfg(test)]
#[path = "tests/protocol.rs"]
mod tests;
