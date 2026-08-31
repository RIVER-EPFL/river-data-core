use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One source-authored annotation to register. `source_key` identifies the
/// annotation within the source system; registration is idempotent per
/// (source_system, source_key), so a full-content pass re-asserting the same
/// key updates in place rather than duplicating.
///
/// The API resolves the site and parameter from the stream's pairing; an
/// annotation on an unpaired stream is reported back as `unpaired` and is
/// re-asserted on a later cycle once the stream is paired.
#[derive(Debug, Clone, Serialize)]
pub struct AnnotationUpsert {
    pub source_key: String,
    pub stream_id: Uuid,
    /// The instant the annotation covers; the API stores it as a point
    /// annotation (start_time == end_time).
    pub time: DateTime<Utc>,
    pub category: String,
    pub text: String,
}

/// The API-side outcome for one registered annotation.
#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationMapping {
    pub source_key: String,
    /// None when the annotation could not be stored (`unpaired`).
    pub id: Option<Uuid>,
    /// created | updated | unchanged | unpaired
    pub status: String,
}
