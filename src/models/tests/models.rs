use super::*;

/// Which shapes refuse a field the sender gained. The API receives all nine and its own
/// structs split the same way: a receiver that refuses answers a version skew with a 400,
/// one that does not drops the field and stores a row missing what the source sent.
#[test]
fn the_shapes_that_refuse_an_unknown_field() {
    fn refuses<D: serde::de::DeserializeOwned>(mut json: serde_json::Value) -> bool {
        json["a_field_the_sender_gained"] = serde_json::json!(1);
        serde_json::from_value::<D>(json).is_err()
    }

    assert!(refuses::<IngestReading>(serde_json::json!({
        "time": "2026-01-15T10:00:00Z", "raw_value": 1.0
    })));
    assert!(refuses::<IngestStatusEvent>(serde_json::json!({
        "time": "2026-01-15T10:00:00Z", "value": "unreachable"
    })));
    assert!(refuses::<SourceWindow>(serde_json::json!({
        "from": "2026-01-01T00:00:00Z", "to": "2026-02-01T00:00:00Z", "source_rows_read": 1
    })));
    assert!(refuses::<GroupAudit>(serde_json::json!({
        "time": "2026-01-15T10:00:00Z"
    })));
    assert!(refuses::<SensorUpsert>(serde_json::json!({
        "source_key": "sensor_inventory:62", "name": "DOC corr", "is_lab_instrument": true
    })));
    assert!(refuses::<AnnotationUpsert>(serde_json::json!({
        "source_key": "annotations:9",
        "stream_id": "00000000-0000-0000-0000-000000000006",
        "time": "2026-01-15T10:00:00Z", "category": "audit", "text": "x"
    })));
    assert!(refuses::<NoteUpsert>(serde_json::json!({
        "source_key": "notes:1", "site_name": "FP1", "text": "x", "verified": true
    })));

    assert!(!refuses::<RegisterStreamRequest>(serde_json::json!({
        "source_system": "cnet", "source_key": "FP1:DOC",
        "source_name": null, "source_path": null, "metadata": {}
    })));
    assert!(!refuses::<StandardCurveUpsert>(serde_json::json!({
        "source_key": "standard_curves:17", "instrument_label": "DOC corr",
        "slope": 1.0, "intercept": 0.0
    })));
    assert!(!refuses::<ColumnAssignment>(serde_json::json!({
        "column": "DOC_A", "index": 0
    })));
}

/// The three fields the API accepts and this crate could not express: a synced instrument's
/// cadence, a portal curve's note, and the instrument a status event describes.
#[test]
fn the_fields_the_api_accepts_travel() {
    let instrument = SensorUpsert {
        source_key: "sensor_inventory:62".to_string(),
        name: "DOC corr".to_string(),
        serial_number: None,
        manufacturer: None,
        model: None,
        notes: None,
        is_lab_instrument: true,
        data_frequency: Some("low".to_string()),
        metadata: None,
    };
    let json = serde_json::to_value(&instrument).unwrap();
    assert_eq!(json["data_frequency"], "low");

    let curve = StandardCurveUpsert {
        source_key: "standard_curves:17".to_string(),
        instrument_label: "DOC corr".to_string(),
        slope: 1.0,
        intercept: 0.0,
        r_squared: None,
        name: None,
        fitted_on: None,
        notes: Some("re-fitted after the lamp change".to_string()),
    };
    let json = serde_json::to_value(&curve).unwrap();
    assert_eq!(json["notes"], "re-fitted after the lamp change");

    let event = IngestStatusEvent {
        time: chrono::Utc::now(),
        value: "unreachable".to_string(),
        sensor_id: Some(uuid::Uuid::nil()),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["sensor_id"], uuid::Uuid::nil().to_string());
}

/// A source that declares none of the three sends none of them, so an API that predates them
/// reads exactly what it read before.
#[test]
fn an_undeclared_field_is_not_sent() {
    let instrument = SensorUpsert {
        source_key: "sensor_inventory:62".to_string(),
        name: "DOC corr".to_string(),
        serial_number: None,
        manufacturer: None,
        model: None,
        notes: None,
        is_lab_instrument: false,
        data_frequency: None,
        metadata: None,
    };
    let json = serde_json::to_value(&instrument).unwrap();
    assert!(json.get("data_frequency").is_none());

    let event = IngestStatusEvent {
        time: chrono::Utc::now(),
        value: "ok".to_string(),
        sensor_id: None,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert!(json.get("sensor_id").is_none());
}
