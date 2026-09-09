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
mod tests {
    use super::*;

    /// The API receives these, so what the client serializes must read back as the same value.
    #[test]
    fn annotation_upsert_round_trips() {
        let up = AnnotationUpsert {
            source_key: "corrections:12".into(),
            stream_id: Uuid::nil(),
            time: Utc::now(),
            category: "audit".into(),
            text: "corrected with the January curve".into(),
            standard_curve_id: Some(Uuid::nil()),
        };
        let json = serde_json::to_value(&up).unwrap();
        let back: AnnotationUpsert = serde_json::from_value(json).unwrap();
        assert_eq!(back.source_key, up.source_key);
        assert_eq!(back.standard_curve_id, up.standard_curve_id);
    }

    #[test]
    fn an_annotation_without_a_curve_round_trips() {
        let up = AnnotationUpsert {
            source_key: "corrections:13".into(),
            stream_id: Uuid::nil(),
            time: Utc::now(),
            category: "audit".into(),
            text: "no curve".into(),
            standard_curve_id: None,
        };
        let json = serde_json::to_value(&up).unwrap();
        assert!(json.get("standard_curve_id").is_none());
        let back: AnnotationUpsert = serde_json::from_value(json).unwrap();
        assert!(back.standard_curve_id.is_none());
    }

    #[test]
    fn note_upsert_round_trips() {
        let up = NoteUpsert {
            source_key: "notes:4".into(),
            site_name: "FP1".into(),
            text: "gauge replaced".into(),
            verified: true,
        };
        let back: NoteUpsert = serde_json::from_value(serde_json::to_value(&up).unwrap()).unwrap();
        assert_eq!(back.site_name, "FP1");
        assert!(back.verified);
    }
}
