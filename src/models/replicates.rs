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
#[serde(deny_unknown_fields)]
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
    /// The portal curve's parameter label. The portal names no instrument for a
    /// curve, so this is where a pairing plan suggests the attachment from; the
    /// API holds the curve until a plan attaches it to one of its instruments.
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
    /// Whatever the source records about the fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// One instrument from a source's own register, to introduce into river-data.
///
/// For a source whose instruments do not each have a stream: every other instrument is minted as a
/// side effect of registering the stream that names it, and a portal's instrument register has no
/// streams to mint from. Registration is idempotent per (source_system, source_key), and a row the
/// API already holds under that key is never rewritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
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
    /// The cadence the instrument logs at ('high' | 'low'), read as a declaration when a stream
    /// classifies its readings. None leaves the API's default of 'high'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_frequency: Option<String>,
    /// Whatever the source knows that river-data has no column for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
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
#[path = "tests/replicates.rs"]
mod tests;
