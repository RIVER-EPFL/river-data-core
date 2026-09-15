use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::annotations::AnnotationUpsert;
use crate::models::replicates::{GroupAudit, ReplicateSpec};
use crate::models::streams::{IngestReading, IngestStatusEvent};

/// Describes a data stream to register with river-data.
#[derive(Debug, Clone)]
pub struct StreamDescriptor {
    /// Unique key within the source system (ie. a location id or column name).
    pub source_key: String,
    /// Human-readable name shown in the dashboard.
    pub source_name: String,
    /// Hierarchy path (ie. "cnet/VAD/WTW_DO_mgL_1"), parsed server-side for site discovery.
    pub source_path: String,
    pub metadata: serde_json::Value,
    /// Stream classification ('spot' or 'continuous'); None defers to the API's resolution chain.
    pub measurement_type: Option<String>,
    /// Owning sensor; required for streams whose readings carry curve claims.
    pub sensor_id: Option<Uuid>,
    /// Replicate-family declaration; requires `measurement_type: "spot"`.
    pub replicates: Option<ReplicateSpec>,
    /// The decimal places the source stores or presents this channel at. Pairing writes it onto
    /// the slot where none is declared, and the public API expresses served values at it. None
    /// leaves the slot undeclared, which is served unrounded.
    pub decimal_places: Option<i16>,
}

/// Asks a backend for readings for one stream since a cursor.
#[derive(Debug, Clone)]
pub struct StreamFetchRequest {
    pub stream_id: Uuid,
    pub source_key: String,
    /// Last known reading time. None on a new stream or a full sync.
    pub since: Option<DateTime<Utc>>,
}

/// A completeness claim over one stream: the readings sent alongside are the COMPLETE content of
/// the source for this stream over `[from, to)`, read from `source_rows_read` source rows. The
/// server diffs stored content against the payload and converges (new / changed / withdrawn);
/// without a window the request is a bare append, exactly the old semantics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceWindow {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// Source rows scanned to produce the payload. An empty payload over a window the store holds
    /// readings for is refused server-side, so a decode failure cannot read as a source deletion.
    pub source_rows_read: u64,
    /// Instants the backend saw but could not carry (cell decode failures). The server retains
    /// stored rows at these keys rather than withdrawing them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_times: Vec<DateTime<Utc>>,
    /// Digest of the canonical payload, stamped by the driver before send. The server persists
    /// it on a cleanly-applied pass and echoes it on the stream list, so the next cycle can skip
    /// re-sending unchanged content. Opaque to the server; never computed server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

/// Readings fetched for one stream, ready to ingest.
#[derive(Debug)]
pub struct StreamReadings {
    pub stream_id: Uuid,
    pub source_key: String,
    pub readings: Vec<IngestReading>,
    /// Portal-precomputed mean/sd per replicate group, for server-side comparison.
    pub audits: Vec<GroupAudit>,
    /// Marks the readings as replicate collections; the API groups them per instant.
    pub collection: bool,
    /// The completeness claim, when this fetch read the source's full content for the stream.
    pub window: Option<SourceWindow>,
    /// Source-authored annotations riding this stream's payload (e.g. the standard curve the
    /// source applied while producing a stored value). The driver registers them after the
    /// stream's readings ingest; idempotent per (source_system, source_key).
    pub annotations: Vec<AnnotationUpsert>,
}

impl StreamReadings {
    /// Plain single-series readings: no audits, not a collection, no completeness claim.
    pub fn new(stream_id: Uuid, source_key: String, readings: Vec<IngestReading>) -> Self {
        Self {
            stream_id,
            source_key,
            readings,
            audits: Vec::new(),
            collection: false,
            window: None,
            annotations: Vec::new(),
        }
    }
}

/// Status events fetched for one stream.
#[derive(Debug)]
pub struct StreamStatusEvents {
    pub stream_id: Uuid,
    pub source_key: String,
    pub events: Vec<IngestStatusEvent>,
}

/// Everything the source holds that could become a stream, whether or not the connector takes it.
///
/// `discover_streams` reports what a connector accepted, so a column it declined and a group it has
/// not discovered yet are invisible to every downstream check: no completeness window covers them,
/// no receipt names them, and reconciliation cannot speak about them at all. This is what a sign-off
/// against a retiring source has to read.
///
/// Taken channels are listed per group because that is the question being asked (is this station,
/// whole, here); declined channels are listed once, source-wide, because a source declines a
/// channel by its own rules and not per group.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SourceInventory {
    /// One entry per channel the connector carries, keyed as it registers.
    pub candidates: Vec<SourceCandidate>,
    /// Channels the source holds and the connector does not carry, with the connector's reason.
    #[serde(default)]
    pub declined: Vec<DeclinedChannel>,
    /// Every group the source holds, so one with no channel at all is still named.
    #[serde(default)]
    pub groups: Vec<String>,
    /// The source's own instrument register, where it keeps one. These are instruments no stream
    /// mints: a portal's `sensor_inventory` is the answer to which probe, which serial, installed
    /// when, and it goes with the portal unless it is admitted here.
    #[serde(default)]
    pub instruments: Vec<crate::models::SensorUpsert>,
}

/// One channel the connector carries.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceCandidate {
    /// The key it registers under.
    pub source_key: String,
    /// The source's own grouping: the station, the location, whatever a report is read by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// One channel the connector leaves behind, and why.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeclinedChannel {
    /// The source's own name for it: a column, a location id.
    pub channel: String,
    pub reason: String,
}

impl SourceInventory {
    /// The inventory a backend that declines nothing has: every descriptor it discovered.
    #[must_use]
    pub fn of_discovered(descriptors: &[StreamDescriptor]) -> Self {
        let candidates: Vec<SourceCandidate> = descriptors
            .iter()
            .map(|d| SourceCandidate {
                source_key: d.source_key.clone(),
                group: d.source_path.split('/').nth(1).map(ToString::to_string),
            })
            .collect();
        let mut groups: Vec<String> = candidates
            .iter()
            .filter_map(|c| c.group.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        groups.sort();
        Self {
            candidates,
            declined: Vec::new(),
            groups,
            instruments: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The completeness claim round-trips: the API echoes it back as `accepted_window` and the
    /// client treats a missing echo as a hard error, so both sides read one shape.
    #[test]
    fn source_window_round_trips() {
        let w = SourceWindow {
            from: Utc::now(),
            to: Utc::now(),
            source_rows_read: 500,
            dropped_times: vec![Utc::now()],
            content_digest: Some("fnv:1".into()),
        };
        let back: SourceWindow = serde_json::from_value(serde_json::to_value(&w).unwrap()).unwrap();
        assert_eq!(back.source_rows_read, 500);
        assert_eq!(back.dropped_times.len(), 1);
        assert_eq!(back.content_digest.as_deref(), Some("fnv:1"));
    }
}
