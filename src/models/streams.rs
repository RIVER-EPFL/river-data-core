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
    /// The authoritative replicate column-to-index mapping, present on the
    /// register and list responses for a stream declaring a replicate family.
    /// Absent on an API that predates it; the same list is then read out of
    /// `metadata.replicates`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<Vec<ColumnAssignment>>,
}

/// How many instruments a source's channels stand for, declared by the connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum InstrumentGranularity {
    /// One instrument per parameter across every site the source reports it at (a field portal).
    PerParameter,
    /// One instrument per site and parameter, stationed there (a logger network).
    PerSiteParameter,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisterStreamRequest {
    pub source_system: String,
    pub source_key: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub metadata: serde_json::Value,
    /// Stream-level classification declared at discovery. None never clears an operator-set value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    /// Owning sensor. Required for curve-carrying streams: the API admits a
    /// reading's curve claim only when reading-sensor == curve-sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    /// Replicate-family declaration; requires `measurement_type: "spot"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<ReplicateSpec>,
    /// The source's decimal places for this channel (0 to 10). None declares nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimal_places: Option<i16>,
    /// The instrument this channel suggests at pairing. None leaves it to the API's inference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_granularity: Option<InstrumentGranularity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IngestReading {
    pub time: chrono::DateTime<chrono::Utc>,
    pub raw_value: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub replicate_index: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<Uuid>,
    /// Per-reading override ('continuous' | 'spot' | 'derived'). None resolves server-side from
    /// the stream default, then the owning sensor's data_frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    /// Standard curve the source applied to this reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IngestStatusEvent {
    pub time: chrono::DateTime<chrono::Utc>,
    pub value: String,
    /// The instrument the status describes, when the source knows it. None leaves the event
    /// attributed to the stream alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API is the receiver of these three, so what the client sends must read back unchanged.
    #[test]
    fn ingest_reading_round_trips() {
        let mut r = IngestReading::new(chrono::Utc::now(), 42.5);
        r.replicate_index = 2;
        r.standard_curve_id = Some(Uuid::nil());
        r.measurement_type = Some("spot".into());
        let back: IngestReading =
            serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
        assert_eq!(back.raw_value, 42.5);
        assert_eq!(back.replicate_index, 2);
        assert_eq!(back.measurement_type.as_deref(), Some("spot"));
        assert_eq!(back.standard_curve_id, Some(Uuid::nil()));
    }

    /// Replicate 0 is omitted on the wire, so the receiver must read an absent index as 0 rather
    /// than refusing the reading.
    #[test]
    fn an_omitted_replicate_index_reads_as_zero() {
        let r = IngestReading::new(chrono::Utc::now(), 1.0);
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("replicate_index").is_none());
        let back: IngestReading = serde_json::from_value(json).unwrap();
        assert_eq!(back.replicate_index, 0);
    }

    #[test]
    fn register_stream_request_round_trips() {
        let req = RegisterStreamRequest {
            source_system: "cnet".to_string(),
            source_key: "FP1:DOC_avg_ppb:reps".to_string(),
            source_name: Some("DOC".to_string()),
            source_path: None,
            metadata: serde_json::json!({"station": "FP1"}),
            measurement_type: Some("spot".to_string()),
            sensor_id: Some(Uuid::nil()),
            replicates: None,
            decimal_places: Some(2),
            instrument_granularity: None,
        };
        let back: RegisterStreamRequest =
            serde_json::from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(back.source_key, req.source_key);
        assert_eq!(back.decimal_places, Some(2));
        assert_eq!(back.sensor_id, Some(Uuid::nil()));
    }

    #[test]
    fn ingest_status_event_round_trips() {
        let e = IngestStatusEvent {
            time: chrono::Utc::now(),
            value: "unreachable".to_string(),
            sensor_id: None,
        };
        let back: IngestStatusEvent =
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
        assert_eq!(back.value, "unreachable");
    }

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
            decimal_places: None,
            instrument_granularity: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["source_system"], "test_system");
        assert_eq!(json["metadata"]["device"], "dev_001");
        assert!(json.get("sensor_id").is_none());
        assert!(json.get("replicates").is_none());
        assert!(json.get("decimal_places").is_none());
    }

    #[test]
    fn test_register_stream_request_declares_decimal_places() {
        let req = RegisterStreamRequest {
            source_system: "cnet".to_string(),
            source_key: "VAD:DOC_rep_1".to_string(),
            source_name: None,
            source_path: None,
            metadata: serde_json::json!({}),
            measurement_type: Some("spot".to_string()),
            sensor_id: None,
            replicates: None,
            decimal_places: Some(2),
            instrument_granularity: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["decimal_places"], 2);
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
            decimal_places: Some(2),
            instrument_granularity: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["measurement_type"], "spot");
        assert_eq!(json["replicates"]["source_columns"][2], "DOC_rep_3");
        assert_eq!(json["replicates"]["curve_ref_column"], "doc_std_curve_id");
    }

    #[test]
    fn test_register_stream_request_declares_instrument_granularity() {
        let req = RegisterStreamRequest {
            source_system: "vaisala".to_string(),
            source_key: "1270".to_string(),
            source_name: None,
            source_path: None,
            metadata: serde_json::json!({}),
            measurement_type: None,
            sensor_id: None,
            replicates: None,
            decimal_places: None,
            instrument_granularity: Some(InstrumentGranularity::PerSiteParameter),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["instrument_granularity"], "per_site_parameter");
        let back: RegisterStreamRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            back.instrument_granularity,
            Some(InstrumentGranularity::PerSiteParameter)
        );
    }

    /// A connector built against a core without the field declares nothing, which the API reads as
    /// its own inference.
    #[test]
    fn test_register_stream_request_without_instrument_granularity() {
        let req: RegisterStreamRequest = serde_json::from_value(serde_json::json!({
            "source_system": "cnet",
            "source_key": "VAD:DOC_rep_1",
            "metadata": {}
        }))
        .unwrap();
        assert_eq!(req.instrument_granularity, None);
        assert!(
            serde_json::to_value(&req)
                .unwrap()
                .get("instrument_granularity")
                .is_none()
        );
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
