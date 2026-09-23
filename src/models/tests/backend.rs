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
