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
