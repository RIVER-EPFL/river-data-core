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
    fn test_sync_event_create_serialization() {
        let ev = SyncEventCreate {
            service_id: Uuid::nil(),
            command_id: None,
            event_type: SyncEventType::Scheduled,
            status: SyncEventStatus::Running,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["event_type"], "scheduled");
        assert_eq!(json["status"], "running");
        assert!(json.get("command_id").is_none());
    }

    #[test]
    fn test_sync_event_update_skips_empty() {
        let upd = SyncEventUpdate {
            status: Some(SyncEventStatus::Completed),
            readings_synced: Some(5),
            ..Default::default()
        };
        let json = serde_json::to_value(&upd).unwrap();
        assert_eq!(json["status"], "completed");
        assert_eq!(json["readings_synced"], 5);
        assert!(json.get("errors").is_none());
        assert!(json.get("duration_ms").is_none());
    }
}
