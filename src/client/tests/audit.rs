use super::{CurveAudit, compare};
use crate::models::{
    DataStream, DeclinedChannel, SourceCandidate, SourceInventory, StandardCurveUpsert,
};
use uuid::Uuid;

fn candidate(key: &str, group: &str) -> SourceCandidate {
    SourceCandidate {
        source_key: key.to_string(),
        group: Some(group.to_string()),
    }
}

fn stream(key: &str, group: &str, paired: bool) -> DataStream {
    DataStream {
        id: Uuid::new_v4(),
        source_system: "cnet".to_string(),
        source_key: key.to_string(),
        source_name: None,
        source_path: Some(format!("cnet/{group}/{key}")),
        metadata: serde_json::json!({}),
        site_parameter_id: paired.then(Uuid::new_v4),
        measurement_type: Some(crate::models::MeasurementType::Spot.to_string()),
        is_active: true,
        last_data_time: None,
        last_window_digest: None,
        replicates: None,
    }
}

fn curve(key: &str) -> StandardCurveUpsert {
    StandardCurveUpsert {
        source_key: key.to_string(),
        instrument_label: "DOC".to_string(),
        slope: 1.0,
        intercept: 0.0,
        r_squared: None,
        name: None,
        fitted_on: None,
        notes: None,
    }
}

fn inventory(candidates: Vec<SourceCandidate>) -> SourceInventory {
    let groups = candidates
        .iter()
        .filter_map(|c| c.group.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    SourceInventory {
        candidates,
        declined: Vec::new(),
        groups,
        instruments: Vec::new(),
    }
}

#[test]
fn test_compare_matches_a_source_that_is_fully_registered() {
    let report = compare(
        "cnet",
        &inventory(vec![
            candidate("VAD:pH", "VAD"),
            candidate("VAD:DOC", "VAD"),
        ]),
        &[
            stream("VAD:pH", "VAD", true),
            stream("VAD:DOC", "VAD", true),
        ],
        &[],
        Some(&[]),
    );
    assert_eq!(report.totals.matched, 2);
    assert_eq!(report.totals.unregistered, 0);
    assert_eq!(report.totals.orphaned, 0);
    assert_eq!(report.totals.unpaired, 0);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].name, "VAD");
}

/// A station added at the portal after the last registration pass is the case the per-cycle
/// reconciliation structurally cannot see: no stream, so no window and no receipt.
#[test]
fn test_compare_reports_a_candidate_with_no_stream() {
    let report = compare(
        "cnet",
        &inventory(vec![candidate("VAD:pH", "VAD"), candidate("FP3:pH", "FP3")]),
        &[stream("VAD:pH", "VAD", true)],
        &[],
        Some(&[]),
    );
    assert_eq!(report.totals.unregistered, 1);
    let fp3 = report.groups.iter().find(|g| g.name == "FP3").unwrap();
    assert_eq!(fp3.unregistered, vec!["FP3:pH".to_string()]);
    assert_eq!(fp3.matched, 0);
}

/// A group the source holds and the connector took nothing from is still named, or a station
/// whose every column was declined would read as a station that does not exist.
#[test]
fn test_compare_names_a_group_with_no_candidates() {
    let mut inv = inventory(vec![candidate("VAD:pH", "VAD")]);
    inv.groups.push("EMPTY".to_string());
    let report = compare("cnet", &inv, &[], &[], Some(&[]));
    let empty = report.groups.iter().find(|g| g.name == "EMPTY").unwrap();
    assert_eq!(empty.candidates, 0);
    assert_eq!(empty.registered, 0);
}

#[test]
fn test_compare_carries_the_declined_channels_source_wide() {
    let mut inv = inventory(vec![candidate("VAD:pH", "VAD")]);
    inv.declined = vec![
        DeclinedChannel {
            channel: "row_id".to_string(),
            reason: "bookkeeping column".to_string(),
        },
        DeclinedChannel {
            channel: "Field_BP".to_string(),
            reason: "not plotted and in no calculation".to_string(),
        },
    ];
    let report = compare("cnet", &inv, &[], &[], Some(&[]));
    assert_eq!(report.totals.declined, 2);
    // Sorted, so two runs of the same source produce the same report.
    assert_eq!(report.declined[0].channel, "Field_BP");
    assert_eq!(report.declined[1].channel, "row_id");
}

#[test]
fn test_compare_reports_an_orphaned_and_an_unpaired_stream() {
    let report = compare(
        "cnet",
        &inventory(vec![candidate("VAD:pH", "VAD")]),
        &[
            stream("VAD:pH", "VAD", false),
            stream("VAD:gone", "VAD", true),
        ],
        &[],
        Some(&[]),
    );
    assert_eq!(report.totals.orphaned, 1);
    assert_eq!(report.totals.unpaired, 1);
    let vad = &report.groups[0];
    assert_eq!(vad.orphaned, vec!["VAD:gone".to_string()]);
    assert_eq!(vad.unpaired, vec!["VAD:pH".to_string()]);
}

#[test]
fn test_compare_reconciles_the_curve_sets_both_ways() {
    let report = compare(
        "cnet",
        &inventory(vec![]),
        &[],
        &[curve("12"), curve("13")],
        Some(&["13".to_string(), "99".to_string()]),
    );
    assert_eq!(
        report.curves,
        CurveAudit {
            at_source: 2,
            registered: 2,
            unregistered: vec!["12".to_string()],
            orphaned: vec!["99".to_string()],
        }
    );
}

/// An API that cannot list curves must not report every source curve as missing.
#[test]
fn test_compare_reports_no_curve_difference_when_the_store_cannot_be_listed() {
    let report = compare("cnet", &inventory(vec![]), &[], &[curve("12")], None);
    assert_eq!(report.curves.at_source, 1);
    assert!(report.curves.unregistered.is_empty());
    assert!(report.curves.orphaned.is_empty());
}

/// A candidate and a stream with no group still have to land somewhere.
#[test]
fn test_compare_groups_ungrouped_entries_under_one_empty_name() {
    let inv = SourceInventory {
        candidates: vec![SourceCandidate {
            source_key: "bare".to_string(),
            group: None,
        }],
        declined: Vec::new(),
        groups: Vec::new(),
        instruments: Vec::new(),
    };
    let mut bare = stream("bare", "", true);
    bare.source_path = None;
    let report = compare("cnet", &inv, &[bare], &[], Some(&[]));
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].name, "");
    assert_eq!(report.groups[0].matched, 1);
}
