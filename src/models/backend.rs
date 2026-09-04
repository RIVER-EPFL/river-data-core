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
