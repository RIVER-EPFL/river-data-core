use chrono::{DateTime, Utc};
use uuid::Uuid;

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
}

/// Asks a backend for readings for one stream since a cursor.
#[derive(Debug, Clone)]
pub struct StreamFetchRequest {
    pub stream_id: Uuid,
    pub source_key: String,
    /// Last known reading time. None on a new stream or a full sync.
    pub since: Option<DateTime<Utc>>,
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
}

impl StreamReadings {
    /// Plain single-series readings: no audits, not a collection.
    pub fn new(stream_id: Uuid, source_key: String, readings: Vec<IngestReading>) -> Self {
        Self {
            stream_id,
            source_key,
            readings,
            audits: Vec::new(),
            collection: false,
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
