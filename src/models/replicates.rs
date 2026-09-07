use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Replicate-family declaration on a stream registration. The API pins each
/// column's replicate_index server-side (append-only across re-registrations)
/// and returns the authoritative mapping as [`ColumnAssignment`]s on the
/// register response; `source_columns` order is provenance, not the index.
/// The API requires at least two unique columns and `measurement_type: "spot"`
/// on the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicateSpec {
    pub source_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portal_mean_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portal_sd_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_ref_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calc: Option<String>,
    /// The sd divisor the source's own sd column uses ('sample' | 'population'),
    /// when the source declares one. Never inferred; None leaves the slot's
    /// declaration (or the audit gate) to decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sd_estimator: Option<String>,
}

/// One source column's pinned replicate index, as the API's register response
/// reports it (and as stream metadata persists it under
/// `replicates.assignments`). Sync services assign each value's
/// `replicate_index` by looking its source column up here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnAssignment {
    pub column: String,
    pub index: i16,
    /// The source no longer sends this column. The index stays reserved and
    /// remains the column's identity should it reappear.
    #[serde(default)]
    pub retired: bool,
}

impl ColumnAssignment {
    /// The pinned mapping a stream's metadata carries, when the API has
    /// authored one. None on metadata written by an API that predates pinning.
    pub fn from_metadata(metadata: &serde_json::Value) -> Option<Vec<Self>> {
        let value = metadata.get("replicates")?.get("assignments")?;
        let assignments: Vec<Self> = serde_json::from_value(value.clone()).ok()?;
        if assignments.is_empty() {
            None
        } else {
            Some(assignments)
        }
    }
}

/// Portal-precomputed mean/sd for one replicate group, sent alongside the
/// group's readings so the API can compare server-side.
#[derive(Debug, Clone, Serialize)]
pub struct GroupAudit {
    pub time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_sd: Option<f64>,
    /// Count of non-null replicate cells the portal row carries for this
    /// instant; the API re-counts after admission, so a divergence surfaces
    /// as an n-mismatch hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_n: Option<i64>,
}

/// One portal standard curve to register. `source_key` identifies the curve
/// within the source system; registration is idempotent per (source_system,
/// source_key).
#[derive(Debug, Clone, Serialize)]
pub struct StandardCurveUpsert {
    pub source_key: String,
    /// The portal curve's parameter label; the API finds-or-creates one lab
    /// instrument per (source_system, instrument_label).
    pub instrument_label: String,
    pub slope: f64,
    pub intercept: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_squared: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The date the source fitted the curve, which is how the lab identifies one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitted_on: Option<chrono::NaiveDate>,
}

/// One instrument from a source's own register, to introduce into river-data.
///
/// For a source whose instruments do not each have a stream: every other instrument is minted as a
/// side effect of registering the stream that names it, and a portal's instrument register has no
/// streams to mint from. Registration is idempotent per (source_system, source_key), and a row the
/// API already holds under that key is never rewritten.
#[derive(Debug, Clone, Serialize)]
pub struct SensorUpsert {
    /// The instrument's identity within the source, e.g. "sensor_inventory:62".
    pub source_key: String,
    pub name: String,
    /// The lab's own serial. The API claims it only when no other instrument holds it, and says
    /// which one does when it declines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// True for an instrument that corrects a grab in the lab rather than standing in a river.
    pub is_lab_instrument: bool,
    /// Whatever the source knows that river-data has no column for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// The API-side identity a registered instrument resolved to.
#[derive(Debug, Clone)]
pub struct SensorMapping {
    pub source_key: String,
    pub id: Uuid,
    /// False when the API already held an instrument under this key.
    pub created: bool,
    /// The instrument already holding the offered serial, when the API declined to claim it.
    pub serial_claimed_by: Option<Uuid>,
}

/// The API-side identity a registered curve resolved to.
#[derive(Debug, Clone)]
pub struct CurveMapping {
    pub source_key: String,
    pub id: Uuid,
    pub sensor_id: Uuid,
    pub superseded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replicate_spec_skips_absent_fields() {
        let spec = ReplicateSpec {
            source_columns: vec!["DIC_A".into(), "DIC_B".into()],
            portal_mean_column: Some("DIC_avg".into()),
            portal_sd_column: None,
            curve_ref_column: None,
            calc: Some("calcMean".into()),
            sd_estimator: None,
        };
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source_columns"][1], "DIC_B");
        assert_eq!(json["portal_mean_column"], "DIC_avg");
        assert!(json.get("portal_sd_column").is_none());
        assert!(json.get("curve_ref_column").is_none());
    }

    #[test]
    fn sensor_upsert_skips_absent_fields() {
        let up = SensorUpsert {
            source_key: "sensor_inventory:62".into(),
            name: "ANU TURB".into(),
            serial_number: Some("919402".into()),
            manufacturer: None,
            model: Some("Cyclops-7".into()),
            notes: None,
            is_lab_instrument: false,
            metadata: None,
        };
        let json = serde_json::to_value(&up).unwrap();
        assert_eq!(json["source_key"], "sensor_inventory:62");
        assert_eq!(json["serial_number"], "919402");
        assert_eq!(json["is_lab_instrument"], false);
        assert!(json.get("manufacturer").is_none());
        assert!(json.get("notes").is_none());
        assert!(json.get("metadata").is_none());
    }

    #[test]
    fn group_audit_skips_absent_fields() {
        let audit = GroupAudit {
            time: Utc::now(),
            expected_mean: Some(1.5),
            expected_sd: None,
            expected_n: None,
        };
        let json = serde_json::to_value(&audit).unwrap();
        assert_eq!(json["expected_mean"], 1.5);
        assert!(json.get("expected_sd").is_none());
        assert!(json.get("expected_n").is_none());
    }

    #[test]
    fn group_audit_serializes_expected_n() {
        let audit = GroupAudit {
            time: Utc::now(),
            expected_mean: Some(1.5),
            expected_sd: Some(0.1),
            expected_n: Some(2),
        };
        let json = serde_json::to_value(&audit).unwrap();
        assert_eq!(json["expected_n"], 2);
    }

    #[test]
    fn column_assignments_parse_from_metadata() {
        let metadata = serde_json::json!({
            "replicates": {
                "source_columns": ["DOC_rep_1", "DOC_rep_3"],
                "assignments": [
                    {"column": "DOC_rep_1", "index": 0},
                    {"column": "DOC_rep_2", "index": 1, "retired": true},
                    {"column": "DOC_rep_3", "index": 2, "retired": false},
                ],
            },
        });
        let assignments = ColumnAssignment::from_metadata(&metadata).unwrap();
        assert_eq!(assignments.len(), 3);
        assert_eq!(assignments[0].column, "DOC_rep_1");
        assert!(!assignments[0].retired);
        assert_eq!(assignments[1].index, 1);
        assert!(assignments[1].retired);
    }

    #[test]
    fn metadata_without_pinned_assignments_yields_none() {
        assert!(ColumnAssignment::from_metadata(&serde_json::json!({})).is_none());
        let unpinned = serde_json::json!({
            "replicates": {"source_columns": ["a", "b"]},
        });
        assert!(ColumnAssignment::from_metadata(&unpinned).is_none());
        let empty = serde_json::json!({
            "replicates": {"source_columns": ["a", "b"], "assignments": []},
        });
        assert!(ColumnAssignment::from_metadata(&empty).is_none());
    }

    #[test]
    fn curve_upsert_serialization() {
        let up = StandardCurveUpsert {
            source_key: "standard_curves:3".into(),
            instrument_label: "DOC corr".into(),
            slope: 1.0,
            intercept: 0.0,
            r_squared: None,
            name: Some("DOC corr 2021-01-28".into()),
            fitted_on: chrono::NaiveDate::from_ymd_opt(2021, 1, 28),
        };
        let json = serde_json::to_value(&up).unwrap();
        assert_eq!(json["source_key"], "standard_curves:3");
        assert_eq!(json["instrument_label"], "DOC corr");
        assert_eq!(json["fitted_on"], "2021-01-28");
        assert!(json.get("r_squared").is_none());
    }
}
