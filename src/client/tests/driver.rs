use chrono::TimeZone;

use super::*;
use crate::models::{IngestReading, SourceWindow};

fn t(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

fn windowed(readings: Vec<IngestReading>) -> StreamReadings {
    let mut sr = StreamReadings::new(Uuid::nil(), "k".to_string(), readings);
    sr.window = Some(SourceWindow {
        from: t(0),
        to: t(1000),
        source_rows_read: 2,
        dropped_times: Vec::new(),
        content_digest: None,
    });
    sr
}

#[test]
fn a_reconciled_backend_reads_from_the_source_start_on_every_cycle() {
    let cursor = Some(t(1_000));
    // The portals are reconciled, so an edit to a row older than the cursor is read on an
    // ordinary cycle and does not wait for the weekly full re-assert.
    assert_eq!(fetch_since(cursor, false, true), None);
    assert_eq!(fetch_since(cursor, true, true), None);
}

#[test]
fn an_append_source_reads_from_its_cursor_until_a_full_pass() {
    let cursor = Some(t(1_000));
    assert_eq!(fetch_since(cursor, false, false), cursor);
    assert_eq!(fetch_since(cursor, true, false), None);
    assert_eq!(fetch_since(None, false, false), None);
}

#[test]
fn digest_is_order_independent() {
    let a = windowed(vec![
        IngestReading::new(t(10), 1.5),
        IngestReading::new(t(20), 2.5),
    ]);
    let b = windowed(vec![
        IngestReading::new(t(20), 2.5),
        IngestReading::new(t(10), 1.5),
    ]);
    assert_eq!(window_digest(&a), window_digest(&b));
}

#[test]
fn digest_changes_with_content() {
    let a = windowed(vec![IngestReading::new(t(10), 1.5)]);
    let mut b = windowed(vec![IngestReading::new(t(10), 1.5001)]);
    assert_ne!(window_digest(&a), window_digest(&b));
    b.readings[0].raw_value = 1.5;
    assert_eq!(window_digest(&a), window_digest(&b));
    b.window.as_mut().unwrap().dropped_times.push(t(30));
    assert_ne!(window_digest(&a), window_digest(&b));
}

#[test]
fn digest_ignores_its_own_field_and_needs_a_window() {
    let a = windowed(vec![IngestReading::new(t(10), 1.5)]);
    let mut b = windowed(vec![IngestReading::new(t(10), 1.5)]);
    b.window.as_mut().unwrap().content_digest = Some("beef".to_string());
    assert_eq!(window_digest(&a), window_digest(&b));
    let bare = StreamReadings::new(Uuid::nil(), "k".to_string(), vec![]);
    assert_eq!(window_digest(&bare), None);
}

/// Scenario: a reconciled backend re-reads its source and finds it unchanged, but stamps the
/// window's `to` at the moment of the scan and reports the whole station's row count.
///
/// Expected behaviour: the digest is over the content the source asserts, so a moving `to` and
/// a row count that follows a sibling column leave it equal and the pass is skipped. The server
/// judges both fields for honesty; neither says anything about this stream's rows.
#[test]
fn digest_ignores_the_scan_clock_and_the_station_row_count() {
    let a = windowed(vec![IngestReading::new(t(10), 1.5)]);
    let mut b = windowed(vec![IngestReading::new(t(10), 1.5)]);
    {
        let w = b.window.as_mut().unwrap();
        w.to = t(9999);
        w.source_rows_read = 501;
    }
    assert_eq!(window_digest(&a), window_digest(&b));

    // `from` is the span the claim covers, so it stays in.
    b.window.as_mut().unwrap().from = t(5);
    assert_ne!(window_digest(&a), window_digest(&b));
}

#[test]
fn digest_sees_annotation_changes() {
    let a = windowed(vec![IngestReading::new(t(10), 1.5)]);
    let mut b = windowed(vec![IngestReading::new(t(10), 1.5)]);
    b.annotations.push(crate::models::AnnotationUpsert {
        source_key: "k:10".to_string(),
        stream_id: Uuid::nil(),
        time: t(10),
        category: "curve".to_string(),
        text: "std curve 7".to_string(),
        standard_curve_id: None,
    });
    assert_ne!(window_digest(&a), window_digest(&b));

    let before = window_digest(&b);
    b.annotations[0].standard_curve_id = Some(Uuid::from_u128(7));
    assert_ne!(
        before,
        window_digest(&b),
        "the curve reference is source-asserted content"
    );
}
