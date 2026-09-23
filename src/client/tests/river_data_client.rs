use super::*;

/// Scenario: a sync registers two curves on a database where a plan has created one of them
/// and not yet attached the other.
///
/// Expected behaviour: the stored curve maps, so readings naming it carry its id; the held one
/// maps to nothing, so its readings travel uncorrected until a plan creates it, and the other
/// curves in the same cycle still map.
#[test]
fn a_curve_held_for_a_plan_maps_to_nothing() {
    let stored: CurveResponse = serde_json::from_value(serde_json::json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "sensor_id": "22222222-2222-2222-2222-222222222222",
        "superseded": false,
        "proposed": false,
    }))
    .unwrap();
    let held: CurveResponse = serde_json::from_value(serde_json::json!({
        "id": null,
        "sensor_id": null,
        "superseded": false,
        "proposed": true,
    }))
    .unwrap();
    let mapped = curve_mapping("standard_curves:1", stored).expect("a stored curve maps");
    assert_eq!(mapped.source_key, "standard_curves:1");
    assert_eq!(
        mapped.id,
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
    );
    assert!(curve_mapping("standard_curves:2", held).is_none());
}

/// Scenario: the API refuses an ingest and says why in the body (a dishonest completeness
/// window, a window on a non-spot stream, a project-scope rejection).
///
/// Expected behaviour: the reason reaches the error, because the error text is what a sync
/// service writes into `sync_events.errors` and that ledger row is the operator's only view of
/// the cycle. A status line alone makes a stream refused for weeks look like a transient 500.
#[tokio::test]
async fn a_refusal_carries_the_servers_explanation() {
    let client = RiverDataClient::new("http://localhost:3000", "tok").unwrap();
    let resp: reqwest::Response = http::Response::builder()
        .status(400)
        .body("a completeness window is only accepted on a stream declared spot")
        .unwrap()
        .into();
    let err = client
        .check_response(resp)
        .await
        .expect_err("a 400 is an error");
    let text = err.to_string();
    assert!(text.contains("400"), "{text}");
    assert!(
        text.contains("only accepted on a stream declared spot"),
        "the server's own words must survive: {text}"
    );
}

/// Scenario: a service whose credential declares another source system offers its instrument
/// register, and the API refuses it with 403 and a JSON error body.
///
/// Expected behaviour: the refusal is an error carrying the server's words, not a count of
/// zero. A zero is what an all-already-admitted cycle reports, so a refusal read as one is a
/// cycle recorded Completed with nothing registered.
#[tokio::test]
async fn a_refused_instrument_register_is_an_error_not_a_count_of_zero() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/sensors/proposals"))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": "this service is enrolled for metalp and cannot register rows as cnet"
            })),
        )
        .mount(&server)
        .await;

    let client = RiverDataClient::new(&server.uri(), "tok").unwrap();
    let err = client
        .propose_instruments("cnet", &[instrument_upsert()])
        .await
        .expect_err("a 403 is an error, not Ok(0)");
    let text = err.to_string();
    assert!(text.contains("403"), "{text}");
    assert!(
        text.contains("enrolled for metalp"),
        "the server's own words must survive: {text}"
    );
}

#[tokio::test]
async fn an_accepted_instrument_register_returns_what_was_stored() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/sensors/proposals"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "stored": 3, "already_admitted": 1 })),
        )
        .mount(&server)
        .await;

    let client = RiverDataClient::new(&server.uri(), "tok").unwrap();
    assert_eq!(
        client
            .propose_instruments("cnet", &[instrument_upsert()])
            .await
            .unwrap(),
        3
    );
}

fn instrument_upsert() -> crate::models::SensorUpsert {
    crate::models::SensorUpsert {
        source_key: "sensor_inventory:62".to_string(),
        name: "DOC corr".to_string(),
        serial_number: None,
        manufacturer: None,
        model: None,
        notes: None,
        is_lab_instrument: true,
        data_frequency: Some("low".to_string()),
        metadata: None,
    }
}

#[tokio::test]
async fn a_success_passes_the_response_through() {
    let client = RiverDataClient::new("http://localhost:3000", "tok").unwrap();
    let resp: reqwest::Response = http::Response::builder()
        .status(200)
        .body("{}")
        .unwrap()
        .into();
    assert!(client.check_response(resp).await.is_ok());
}

#[test]
fn a_long_body_is_clipped_rather_than_filling_the_ledger_row() {
    let clipped = truncate(&"x".repeat(900), 500);
    assert_eq!(clipped.chars().count(), 501, "500 characters plus the mark");
    assert!(clipped.ends_with('…'));
    // Short text is returned whole, and clipping never splits a character.
    assert_eq!(truncate("short", 500), "short");
    assert_eq!(truncate("é".repeat(10).as_str(), 3), "ééé…");
}

#[test]
fn test_url_construction() {
    let client = RiverDataClient::new("http://localhost:3000", "tok").unwrap();
    assert_eq!(
        client.url("/data_streams"),
        "http://localhost:3000/api/data_streams"
    );
    assert_eq!(client.url("/ingest"), "http://localhost:3000/api/ingest");
}

#[test]
fn test_url_strips_trailing_slash() {
    let client = RiverDataClient::new("http://localhost:3000/", "tok").unwrap();
    assert_eq!(
        client.url("/data_streams"),
        "http://localhost:3000/api/data_streams"
    );
}

#[test]
fn test_parse_content_range_total() {
    let resp = http::Response::builder()
        .header("content-range", "data_streams 0-999/29400")
        .body("")
        .unwrap();
    let resp: reqwest::Response = resp.into();
    assert_eq!(
        RiverDataClient::parse_content_range_total(&resp),
        Some(29400)
    );

    let resp = http::Response::builder()
        .header("content-range", "data_streams 0-21/22")
        .body("")
        .unwrap();
    let resp: reqwest::Response = resp.into();
    assert_eq!(RiverDataClient::parse_content_range_total(&resp), Some(22));

    let resp = http::Response::builder().body("").unwrap();
    let resp: reqwest::Response = resp.into();
    assert_eq!(RiverDataClient::parse_content_range_total(&resp), None);
}

fn reading_at(secs: i64, idx: i16) -> IngestReading {
    IngestReading {
        replicate_index: idx,
        ..IngestReading::new(
            chrono::DateTime::from_timestamp(secs, 0).unwrap(),
            secs as f64,
        )
    }
}

#[test]
fn chunks_respect_batch_size_on_distinct_timestamps() {
    let readings: Vec<_> = (0..10).map(|s| reading_at(s, 0)).collect();
    let chunks = group_safe_chunks(&readings, 4);
    assert_eq!(
        chunks.iter().map(|c| c.len()).collect::<Vec<_>>(),
        vec![4, 4, 2]
    );
}

#[test]
fn a_replicate_group_is_never_split_across_chunks() {
    // Groups: t0 (1 row), t1 (3 rows), t2 (2 rows). Batch size 3 would cut
    // the t1 group after its second member.
    let readings = vec![
        reading_at(0, 0),
        reading_at(1, 0),
        reading_at(1, 1),
        reading_at(1, 2),
        reading_at(2, 0),
        reading_at(2, 1),
    ];
    let chunks = group_safe_chunks(&readings, 3);
    for chunk in &chunks {
        let first = chunk[0].time;
        let last = chunk[chunk.len() - 1].time;
        for other in &chunks {
            if !std::ptr::eq(*chunk, *other) {
                for r in *other {
                    assert!(
                        r.time != first && r.time != last,
                        "timestamp run split across chunks"
                    );
                }
            }
        }
    }
    assert_eq!(
        chunks.iter().map(|c| c.len()).collect::<Vec<_>>(),
        vec![1, 3, 2]
    );
}

#[test]
fn a_group_larger_than_the_batch_size_is_one_oversized_chunk() {
    let readings: Vec<_> = (0..5).map(|i| reading_at(7, i)).collect();
    let chunks = group_safe_chunks(&readings, 3);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 5);
}

#[test]
fn the_cut_moves_before_a_run_that_spans_the_boundary() {
    let readings = vec![
        reading_at(0, 0),
        reading_at(0, 1),
        reading_at(1, 0),
        reading_at(1, 1),
        reading_at(1, 2),
    ];
    let chunks = group_safe_chunks(&readings, 3);
    assert_eq!(
        chunks.iter().map(|c| c.len()).collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn empty_input_yields_no_chunks() {
    assert!(group_safe_chunks(&[], 100).is_empty());
}

#[test]
fn test_token_set_and_get() {
    let client = RiverDataClient::new("http://localhost:3000", "initial").unwrap();
    assert_eq!(client.current_token(), "initial");

    client.set_token("rotated");
    assert_eq!(client.current_token(), "rotated");
}

#[test]
fn test_concurrent_token_access() {
    use std::sync::Arc;
    let client = Arc::new(RiverDataClient::new("http://localhost:3000", "v1").unwrap());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let c = client.clone();
            std::thread::spawn(move || {
                c.set_token(&format!("v{i}"));
                let _ = c.current_token();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let token = client.current_token();
    assert!(token.starts_with('v'), "unexpected token: {token}");
}
