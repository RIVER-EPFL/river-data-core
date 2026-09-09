//! The source audit: everything the source holds, against everything registered here.
//!
//! Per-cycle reconciliation can only speak about streams that exist. A channel the connector
//! declined, a group discovered after the last registration pass, and a stream whose key the source
//! no longer offers are outside every completeness window and named on no receipt, so their absence
//! reads as clean. This is the comparison a sign-off against a retiring source rests on, and it is
//! read per group rather than per pass because that is the question being asked: is this station,
//! whole, here.
//!
//! It writes nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::models::{DataStream, SourceInventory, StandardCurveUpsert};

/// One source's standing against the store.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceAuditReport {
    pub source_system: String,
    pub totals: AuditTotals,
    /// One entry per group, ordered by name. A candidate or stream with no group falls under `""`,
    /// so nothing is dropped for lacking one.
    pub groups: Vec<SourceGroupReport>,
    /// Channels the connector does not carry, source-wide: a source declines by its own rules and
    /// not per group, so listing these once is the whole of it.
    pub declined: Vec<crate::models::DeclinedChannel>,
    pub curves: CurveAudit,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AuditTotals {
    /// Channels the connector carries.
    pub candidates: usize,
    /// Channels the source holds and the connector does not.
    pub declined: usize,
    pub registered: usize,
    /// Candidates with a registered stream: the healthy case.
    pub matched: usize,
    /// Candidates with no stream here, which no window or receipt can report.
    pub unregistered: usize,
    /// Registered streams the connector no longer offers.
    pub orphaned: usize,
    /// Registered streams with no site parameter, whose readings are attributed to nothing.
    pub unpaired: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceGroupReport {
    pub name: String,
    pub candidates: usize,
    pub registered: usize,
    pub matched: usize,
    /// Source keys the connector carries and nothing here holds.
    pub unregistered: Vec<String>,
    /// Registered source keys the connector no longer offers.
    pub orphaned: Vec<String>,
    /// Registered source keys with no pairing.
    pub unpaired: Vec<String>,
}

/// Lab curves replicate by `(source_system, source_key)` and nothing else re-compares the two sets.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurveAudit {
    pub at_source: usize,
    pub registered: usize,
    /// Source keys the source holds and the store does not.
    pub unregistered: Vec<String>,
    /// Source keys the store holds and the source no longer offers.
    pub orphaned: Vec<String>,
}

/// The group a registered stream belongs to: its hierarchy path's second segment
/// (`{source_system}/{group}/{channel}`), which is how every backend here builds one.
fn stream_group(stream: &DataStream) -> String {
    stream
        .source_path
        .as_deref()
        .and_then(|p| p.split('/').nth(1))
        .unwrap_or_default()
        .to_string()
}

/// Compare one source's inventory and curves against what is registered here.
///
/// `registered_curve_keys` is the store's `(source_system, source_key)` set for this source; an API
/// that cannot list them passes `None`, and the curve arms then report only what the source holds
/// rather than declaring every curve missing.
#[must_use]
pub fn compare(
    source_system: &str,
    inventory: &SourceInventory,
    registered: &[DataStream],
    source_curves: &[StandardCurveUpsert],
    registered_curve_keys: Option<&[String]>,
) -> SourceAuditReport {
    let registered_keys: BTreeSet<&str> =
        registered.iter().map(|s| s.source_key.as_str()).collect();
    let candidate_keys: BTreeSet<&str> = inventory
        .candidates
        .iter()
        .map(|c| c.source_key.as_str())
        .collect();

    let mut groups: BTreeMap<String, SourceGroupReport> = inventory
        .groups
        .iter()
        .map(|g| (g.clone(), SourceGroupReport::empty(g)))
        .collect();

    let mut totals = AuditTotals {
        candidates: inventory.candidates.len(),
        declined: inventory.declined.len(),
        registered: registered.len(),
        ..AuditTotals::default()
    };

    for candidate in &inventory.candidates {
        let name = candidate.group.clone().unwrap_or_default();
        let entry = groups
            .entry(name.clone())
            .or_insert_with(|| SourceGroupReport::empty(&name));
        entry.candidates += 1;
        if registered_keys.contains(candidate.source_key.as_str()) {
            totals.matched += 1;
            entry.matched += 1;
        } else {
            totals.unregistered += 1;
            entry.unregistered.push(candidate.source_key.clone());
        }
    }

    for stream in registered {
        let name = stream_group(stream);
        let entry = groups
            .entry(name.clone())
            .or_insert_with(|| SourceGroupReport::empty(&name));
        entry.registered += 1;
        if !candidate_keys.contains(stream.source_key.as_str()) {
            totals.orphaned += 1;
            entry.orphaned.push(stream.source_key.clone());
        }
        if stream.site_parameter_id.is_none() {
            totals.unpaired += 1;
            entry.unpaired.push(stream.source_key.clone());
        }
    }

    let source_curve_keys: BTreeSet<&str> = source_curves
        .iter()
        .map(|c| c.source_key.as_str())
        .collect();
    let curves = match registered_curve_keys {
        None => CurveAudit {
            at_source: source_curve_keys.len(),
            ..CurveAudit::default()
        },
        Some(stored) => {
            let stored_set: BTreeSet<&str> = stored.iter().map(String::as_str).collect();
            CurveAudit {
                at_source: source_curve_keys.len(),
                registered: stored_set.len(),
                unregistered: source_curve_keys
                    .difference(&stored_set)
                    .map(|k| (*k).to_string())
                    .collect(),
                orphaned: stored_set
                    .difference(&source_curve_keys)
                    .map(|k| (*k).to_string())
                    .collect(),
            }
        }
    };

    let mut declined = inventory.declined.clone();
    declined.sort_by(|a, b| a.channel.cmp(&b.channel));

    SourceAuditReport {
        source_system: source_system.to_string(),
        totals,
        groups: groups
            .into_values()
            .map(|mut group| {
                group.unregistered.sort();
                group.orphaned.sort();
                group.unpaired.sort();
                group
            })
            .collect(),
        declined,
        curves,
    }
}

impl SourceGroupReport {
    fn empty(name: &str) -> Self {
        Self {
            name: name.to_string(),
            candidates: 0,
            registered: 0,
            matched: 0,
            unregistered: Vec::new(),
            orphaned: Vec::new(),
            unpaired: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
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
        };
        let mut bare = stream("bare", "", true);
        bare.source_path = None;
        let report = compare("cnet", &inv, &[bare], &[], Some(&[]));
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].name, "");
        assert_eq!(report.groups[0].matched, 1);
    }
}
