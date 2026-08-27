use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Replicate-family declaration on a stream registration. `source_columns`
/// position is the replicate_index the member's readings carry, and the API
/// preserves it unchanged; the API requires at least two unique columns and
/// `measurement_type: "spot"` on the request.
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
        };
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["source_columns"][1], "DIC_B");
        assert_eq!(json["portal_mean_column"], "DIC_avg");
        assert!(json.get("portal_sd_column").is_none());
        assert!(json.get("curve_ref_column").is_none());
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
    fn curve_upsert_serialization() {
        let up = StandardCurveUpsert {
            source_key: "standard_curves:3".into(),
            instrument_label: "DOC corr".into(),
            slope: 1.0,
            intercept: 0.0,
            r_squared: None,
            name: Some("DOC corr 2021-01-28".into()),
        };
        let json = serde_json::to_value(&up).unwrap();
        assert_eq!(json["source_key"], "standard_curves:3");
        assert_eq!(json["instrument_label"], "DOC corr");
        assert!(json.get("r_squared").is_none());
    }
}
