use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::replicates::{ColumnAssignment, ReplicateSpec};

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
    /// Content digest of the last cleanly-applied windowed pass, as claimed by the sync client.
    /// Absent on APIs that predate the handshake; the client then sends full windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_window_digest: Option<String>,
    /// The authoritative replicate column-to-index mapping, present on a
    /// register response for a stream declaring a replicate family. Absent on
    /// list responses and on APIs that predate pinning; the same list persists
    /// under `metadata.replicates.assignments`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<Vec<ColumnAssignment>>,
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
    /// Owning sensor. Required for curve-carrying streams: the API admits a
    /// reading's curve claim only when reading-sensor == curve-sensor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    /// Replicate-family declaration; requires `measurement_type: "spot"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicates: Option<ReplicateSpec>,
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
    /// Standard curve the source applied to this reading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_curve_id: Option<Uuid>,
}

impl IngestReading {
    /// A reading at replicate 0 with no sensor attribution; the server resolves the rest.
    pub fn new(time: chrono::DateTime<chrono::Utc>, raw_value: f64) -> Self {
        Self {
            time,
            raw_value,
            replicate_index: 0,
            sensor_id: None,
            calibration_id: None,
            deployment_id: None,
            measurement_type: None,
            standard_curve_id: None,
        }
    }
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
    fn test_ingest_reading_serialization() {
        let r = IngestReading::new(chrono::Utc::now(), 42.5);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["raw_value"], 42.5);
        assert!(json.get("replicate_index").is_none());
        assert!(json.get("sensor_id").is_none());
        assert!(json.get("measurement_type").is_none());
    }

    #[test]
    fn test_register_stream_request() {
        let req = RegisterStreamRequest {
            source_system: "test_system".to_string(),
            source_key: "source_1".to_string(),
            source_name: Some("stream_a".to_string()),
            source_path: None,
            metadata: serde_json::json!({"device": "dev_001"}),
            measurement_type: None,
            sensor_id: None,
            replicates: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["source_system"], "test_system");
        assert_eq!(json["metadata"]["device"], "dev_001");
        assert!(json.get("sensor_id").is_none());
        assert!(json.get("replicates").is_none());
    }

    #[test]
    fn test_register_stream_request_with_replicates() {
        let req = RegisterStreamRequest {
            source_system: "cnet".to_string(),
            source_key: "VAD:DOC_avg_ppb:reps".to_string(),
            source_name: None,
            source_path: None,
            metadata: serde_json::json!({}),
            measurement_type: Some("spot".to_string()),
            sensor_id: Some(Uuid::nil()),
            replicates: Some(crate::models::replicates::ReplicateSpec {
                source_columns: vec!["DOC_rep_1".into(), "DOC_rep_2".into(), "DOC_rep_3".into()],
                portal_mean_column: Some("DOC_avg_ppb".into()),
                portal_sd_column: Some("DOC_sd_ppb".into()),
                curve_ref_column: Some("doc_std_curve_id".into()),
                calc: Some("calcDOCavg".into()),
                sd_estimator: None,
            }),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["measurement_type"], "spot");
        assert_eq!(json["replicates"]["source_columns"][2], "DOC_rep_3");
        assert_eq!(json["replicates"]["curve_ref_column"], "doc_std_curve_id");
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
        assert!(stream.replicates.is_none());
    }

    #[test]
    fn register_response_replicates_parse() {
        let json = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "source_system": "cnet",
            "source_key": "VAD:DOC_avg_ppb:reps",
            "source_name": null,
            "source_path": null,
            "metadata": {},
            "site_parameter_id": null,
            "is_active": true,
            "last_data_time": null,
            "replicates": [
                {"column": "DOC_rep_1", "index": 0},
                {"column": "DOC_rep_2", "index": 5, "retired": true},
            ]
        });
        let stream: DataStream = serde_json::from_value(json).unwrap();
        let assignments = stream.replicates.unwrap();
        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].index, 0);
        assert!(!assignments[0].retired);
        assert_eq!(assignments[1].column, "DOC_rep_2");
        assert_eq!(assignments[1].index, 5);
        assert!(assignments[1].retired);
    }
}
