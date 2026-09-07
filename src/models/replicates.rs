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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ColumnAssignment {
    pub column: String,
    pub index: i16,
    /// The source no longer sends this column. The index stays reserved and
    /// remains the column's identity should it reappear.
    #[serde(default)]
    pub retired: bool,
}

impl ColumnAssignment {
    /// The mapping a stream's metadata carries, resolved the way the API
    /// resolves it. None when the stream declares no replicate family.
    pub fn from_metadata(metadata: &serde_json::Value) -> Option<Vec<Self>> {
        let spec = metadata.get("replicates")?;
        let pinned: Vec<Self> = spec
            .get("assignments")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let source_columns: Vec<String> = spec
            .get("source_columns")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let resolved = Self::resolve(pinned, &source_columns);
        (!resolved.is_empty()).then_some(resolved)
    }

    /// The authoritative mapping, ordered by index: the assignments the API
    /// pinned, or, on a spec stored before pinning, each declared source column
    /// at its position, which is the index its readings were stored under.
    /// Both crates read an unpinned spec through here so that neither invents
    /// an index the other would not.
    #[must_use]
    pub fn resolve(pinned: Vec<Self>, source_columns: &[String]) -> Vec<Self> {
        let mut resolved = if pinned.is_empty() {
            source_columns
                .iter()
                .enumerate()
                .map(|(i, column)| Self {
                    column: column.clone(),
                    index: i16::try_from(i).unwrap_or(i16::MAX),
                    retired: false,
                })
                .collect()
        } else {
            pinned
        };
        resolved.sort_by_key(|a| a.index);
        resolved
    }
}

/// Portal-precomputed mean/sd for one replicate group, sent alongside the
/// group's readings so the API can compare server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GroupAudit {
    pub time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_mean: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StandardCurveUpsert {
    pub source_key: String,
    /// The portal curve's parameter label; the API finds-or-creates one lab
    /// instrument per (source_system, instrument_label).
    pub instrument_label: String,
    pub slope: f64,
    pub intercept: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_squared: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The date the source fitted the curve, which is how the lab identifies one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fitted_on: Option<chrono::NaiveDate>,
}

/// One instrument from a source's own register, to introduce into river-data.
///
/// For a source whose instruments do not each have a stream: every other instrument is minted as a
/// side effect of registering the stream that names it, and a portal's instrument register has no
/// streams to mint from. Registration is idempotent per (source_system, source_key), and a row the
/// API already holds under that key is never rewritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SensorUpsert {
    /// The instrument's identity within the source, e.g. "sensor_inventory:62".
    pub source_key: String,
    pub name: String,
    /// The lab's own serial. The API claims it only when no other instrument holds it, and says
    /// which one does when it declines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// True for an instrument that corrects a grab in the lab rather than standing in a river.
    pub is_lab_instrument: bool,
    /// Whatever the source knows that river-data has no column for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    /// The API receives the declaration, the pinned mapping, the audit expectations and both
    /// registers, so each must read back as what the client sent.
    #[test]
    fn the_registered_wire_types_round_trip() {
        let spec = ReplicateSpec {
            source_columns: vec!["DOC_rep_1".into(), "DOC_rep_2".into()],
            portal_mean_column: Some("DOC_avg".into()),
            portal_sd_column: None,
            curve_ref_column: None,
            calc: Some("calcMean".into()),
            sd_estimator: Some("population".into()),
        };
        let back: ReplicateSpec =
            serde_json::from_value(serde_json::to_value(&spec).unwrap()).unwrap();
        assert_eq!(back.source_columns, spec.source_columns);
        assert_eq!(back.sd_estimator.as_deref(), Some("population"));

        let audit = GroupAudit {
            time: Utc::now(),
            expected_mean: Some(1.5),
            expected_sd: Some(0.1),
            expected_n: Some(3),
        };
        let back: GroupAudit =
            serde_json::from_value(serde_json::to_value(&audit).unwrap()).unwrap();
        assert_eq!(back.expected_n, Some(3));

        let curve = StandardCurveUpsert {
            source_key: "standard_curves:3".into(),
            instrument_label: "DOC corr".into(),
            slope: 1.0,
            intercept: 0.0,
            r_squared: None,
            name: Some("DOC corr 2021-01-28".into()),
            fitted_on: chrono::NaiveDate::from_ymd_opt(2021, 1, 28),
        };
        let back: StandardCurveUpsert =
            serde_json::from_value(serde_json::to_value(&curve).unwrap()).unwrap();
        assert_eq!(back.fitted_on, curve.fitted_on);
        assert!(back.r_squared.is_none());

        let sensor = SensorUpsert {
            source_key: "sensor_inventory:62".into(),
            name: "ANU TURB".into(),
            serial_number: Some("919402".into()),
            manufacturer: None,
            model: Some("Cyclops-7".into()),
            notes: None,
            is_lab_instrument: false,
            metadata: None,
        };
        let back: SensorUpsert =
            serde_json::from_value(serde_json::to_value(&sensor).unwrap()).unwrap();
        assert_eq!(back.serial_number.as_deref(), Some("919402"));
        assert!(!back.is_lab_instrument);

        let assignment = ColumnAssignment {
            column: "DOC_rep_2".into(),
            index: 1,
            retired: true,
        };
        let back: ColumnAssignment =
            serde_json::from_value(serde_json::to_value(&assignment).unwrap()).unwrap();
        assert_eq!(back, assignment);
    }

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
    fn metadata_without_a_replicate_spec_yields_none() {
        assert!(ColumnAssignment::from_metadata(&serde_json::json!({})).is_none());
        let no_columns = serde_json::json!({ "replicates": {"source_columns": []} });
        assert!(ColumnAssignment::from_metadata(&no_columns).is_none());
    }

    /// An unpinned spec means the same thing on both sides of the wire: each
    /// declared column at its position, which is what the readings registered
    /// before pinning carry. The API resolves the metadata it stores the same
    /// way, so neither crate can index a value the other would index
    /// differently.
    #[test]
    fn an_unpinned_spec_resolves_to_column_positions() {
        for spec in [
            serde_json::json!({"source_columns": ["DOC_rep_1", "DOC_rep_2", "DOC_rep_3"]}),
            serde_json::json!({
                "source_columns": ["DOC_rep_1", "DOC_rep_2", "DOC_rep_3"],
                "assignments": [],
            }),
        ] {
            let metadata = serde_json::json!({ "replicates": spec });
            let assignments = ColumnAssignment::from_metadata(&metadata).unwrap();
            assert_eq!(
                assignments,
                vec![
                    ColumnAssignment {
                        column: "DOC_rep_1".into(),
                        index: 0,
                        retired: false,
                    },
                    ColumnAssignment {
                        column: "DOC_rep_2".into(),
                        index: 1,
                        retired: false,
                    },
                    ColumnAssignment {
                        column: "DOC_rep_3".into(),
                        index: 2,
                        retired: false,
                    },
                ]
            );
        }
    }

    #[test]
    fn pinned_assignments_win_over_column_order() {
        let pinned = vec![
            ColumnAssignment {
                column: "DOC_rep_2".into(),
                index: 1,
                retired: false,
            },
            ColumnAssignment {
                column: "DOC_rep_1".into(),
                index: 0,
                retired: false,
            },
        ];
        let resolved = ColumnAssignment::resolve(
            pinned,
            &["DOC_rep_1".to_string(), "DOC_rep_2".to_string()],
        );
        assert_eq!(resolved[0].column, "DOC_rep_1");
        assert_eq!(resolved[1].index, 1);
    }

    /// A column the source stopped sending keeps its index, and the index stays out of use. The
    /// API pins that state; a client that dropped it, or that renumbered around the gap, would
    /// store the surviving columns' readings under indexes the store already gave to others.
    #[test]
    fn a_retired_column_keeps_its_index_and_the_gap_it_leaves() {
        let metadata = serde_json::json!({ "replicates": {
            "source_columns": ["DOC_rep_A", "DOC_rep_C"],
            "assignments": [
                { "column": "DOC_rep_C", "index": 2 },
                { "column": "DOC_rep_A", "index": 0 },
                { "column": "DOC_rep_B", "index": 1, "retired": true },
            ],
        }});
        let assignments = ColumnAssignment::from_metadata(&metadata).unwrap();
        assert_eq!(
            assignments,
            vec![
                ColumnAssignment {
                    column: "DOC_rep_A".into(),
                    index: 0,
                    retired: false,
                },
                ColumnAssignment {
                    column: "DOC_rep_B".into(),
                    index: 1,
                    retired: true,
                },
                ColumnAssignment {
                    column: "DOC_rep_C".into(),
                    index: 2,
                    retired: false,
                },
            ],
            "the retired column is kept, in its place, and nothing is renumbered over its index"
        );
    }

    /// The declared columns are not the mapping when the API has pinned one: a spec whose
    /// `source_columns` disagree with its pinned indexes resolves to the pinned indexes, so the
    /// two crates cannot land on different positions for the same column.
    #[test]
    fn pinning_beats_the_declared_column_list() {
        let metadata = serde_json::json!({ "replicates": {
            "source_columns": ["DOC_rep_B", "DOC_rep_A"],
            "assignments": [
                { "column": "DOC_rep_A", "index": 0 },
                { "column": "DOC_rep_B", "index": 1 },
            ],
        }});
        let assignments = ColumnAssignment::from_metadata(&metadata).unwrap();
        assert_eq!(
            assignments.iter().map(|a| a.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(assignments[0].column, "DOC_rep_A");
    }

    /// A family that declares nothing and has nothing pinned is not a mapping of zero columns; it
    /// is no mapping, and a caller must not read it as "index every column at 0".
    #[test]
    fn an_empty_spec_resolves_to_no_mapping() {
        let metadata = serde_json::json!({ "replicates": { "source_columns": [] } });
        assert!(ColumnAssignment::from_metadata(&metadata).is_none());
        assert!(ColumnAssignment::resolve(Vec::new(), &[]).is_empty());
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
