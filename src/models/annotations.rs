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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AnnotationUpsert {
    pub source_key: String,
    pub stream_id: Uuid,
    /// The instant the annotation covers; the API stores it as a point
    /// annotation (start_time == end_time).
    pub time: DateTime<Utc>,
    pub category: String,
    pub text: String,
    /// The standard curve the source applied to produce the annotated value. The API freezes an
    /// annotation's curve and text once stored with one, reporting later edits as `frozen`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_curve_id: Option<Uuid>,
}

/// The API-side outcome for one registered annotation.
#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationMapping {
    pub source_key: String,
    /// None when the annotation could not be stored (`unpaired`).
    pub id: Option<Uuid>,
    /// created | updated | unchanged | frozen | unpaired
    pub status: String,
}

/// One source-authored site note to register. `source_key` identifies the note
/// within the source system; registration is idempotent per
/// (source_system, source_key).
///
/// `site_name` is the source's own station name, which the API resolves against
/// sites that already exist. A note mints nothing: one naming a station
/// river-data has never seen is reported `unresolved` and is re-asserted on a
/// later cycle, once pairing has created the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct NoteUpsert {
    pub source_key: String,
    pub site_name: String,
    pub text: String,
    pub verified: bool,
}

/// The API-side outcome for one registered note.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteMapping {
    pub source_key: String,
    /// None when the note could not be stored (`unresolved`).
    pub id: Option<Uuid>,
    /// created | updated | unchanged | unresolved
    pub status: String,
}

#[cfg(test)]
#[path = "tests/annotations.rs"]
mod tests;
