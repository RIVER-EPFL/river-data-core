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
