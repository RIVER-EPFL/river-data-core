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
    };
    let back: ReplicateSpec = serde_json::from_value(serde_json::to_value(&spec).unwrap()).unwrap();
    assert_eq!(back.source_columns, spec.source_columns);

    let audit = GroupAudit {
        time: Utc::now(),
        expected_mean: Some(1.5),
        expected_sd: Some(0.1),
        expected_n: Some(3),
    };
    let back: GroupAudit = serde_json::from_value(serde_json::to_value(&audit).unwrap()).unwrap();
    assert_eq!(back.expected_n, Some(3));

    let curve = StandardCurveUpsert {
        source_key: "standard_curves:3".into(),
        instrument_label: "DOC corr".into(),
        slope: 1.0,
        intercept: 0.0,
        r_squared: None,
        name: Some("DOC corr 2021-01-28".into()),
        fitted_on: chrono::NaiveDate::from_ymd_opt(2021, 1, 28),
        notes: None,
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
        data_frequency: None,
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
fn test_replicate_spec_drops_sd_estimator() {
    let json = serde_json::json!({
        "source_columns": ["DOC_rep_1", "DOC_rep_2"],
        "sd_estimator": "population",
    });
    let spec: ReplicateSpec = serde_json::from_value(json).unwrap();
    assert!(
        serde_json::to_value(&spec)
            .unwrap()
            .get("sd_estimator")
            .is_none()
    );
}

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
fn sensor_upsert_skips_absent_fields() {
    let up = SensorUpsert {
        source_key: "sensor_inventory:62".into(),
        name: "ANU TURB".into(),
        serial_number: Some("919402".into()),
        manufacturer: None,
        model: Some("Cyclops-7".into()),
        notes: None,
        is_lab_instrument: false,
        data_frequency: None,
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
    let resolved =
        ColumnAssignment::resolve(pinned, &["DOC_rep_1".to_string(), "DOC_rep_2".to_string()]);
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
        notes: None,
    };
    let json = serde_json::to_value(&up).unwrap();
    assert_eq!(json["source_key"], "standard_curves:3");
    assert_eq!(json["instrument_label"], "DOC corr");
    assert_eq!(json["fitted_on"], "2021-01-28");
    assert!(json.get("r_squared").is_none());
}
