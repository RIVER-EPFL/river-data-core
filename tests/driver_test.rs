#![cfg(feature = "client")]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use river_data_core::client::{
    BackendError, RiverDataClient, SourceBackend, StreamDescriptor, StreamFetchRequest,
    StreamReadings, SyncDriver, SyncService,
};
use river_data_core::models::{ColumnAssignment, IngestReading, RunnerConfig, SourceWindow};

fn test_config() -> RunnerConfig {
    RunnerConfig {
        api_base_url: String::new(),
        client_id: "svc_test".to_string(),
        client_secret: "secret".to_string(),
        instance_id: "test".to_string(),
        heartbeat_interval_secs: 30,
        sync_interval_secs: 300,
        enrollment_retry_secs: 1,
        retry_max: 3,
        retry_delay_secs: 0,
    }
}

fn stream_json(id: Uuid, source_key: &str, last_data_time: Option<&str>) -> serde_json::Value {
    json!({
        "id": id,
        "source_system": "fake",
        "source_key": source_key,
        "source_name": source_key,
        "source_path": format!("fake/{source_key}"),
        "metadata": {},
        "site_parameter_id": null,
        "is_active": true,
        "last_data_time": last_data_time,
    })
}

type AppliedAssignments = Arc<Mutex<Vec<(String, Vec<ColumnAssignment>)>>>;

#[derive(Default)]
struct FakeBackend {
    readings_per_stream: usize,
    fail_fetch: bool,
    rediscover: bool,
    reconciled: bool,
    windowed: bool,
    fetch_calls: Arc<AtomicU32>,
    fetch_requests: Arc<Mutex<Vec<Vec<StreamFetchRequest>>>>,
    applied_assignments: AppliedAssignments,
}

#[async_trait::async_trait]
impl SourceBackend for FakeBackend {
    fn source_system(&self) -> &str {
        "fake"
    }

    fn rediscover_every_cycle(&self) -> bool {
        self.rediscover
    }

    fn reconciled(&self) -> bool {
        self.reconciled
    }

    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
        Ok(vec![StreamDescriptor {
            source_key: "s1".to_string(),
            source_name: "Stream 1".to_string(),
            source_path: "fake/s1".to_string(),
            metadata: json!({}),
            measurement_type: Some("continuous".to_string()),
            sensor_id: None,
            replicates: None,
            decimal_places: None,
        }])
    }

    async fn apply_replicate_assignments(
        &self,
        source_key: &str,
        assignments: &[ColumnAssignment],
    ) -> Result<(), BackendError> {
        self.applied_assignments
            .lock()
            .unwrap()
            .push((source_key.to_string(), assignments.to_vec()));
        Ok(())
    }

    async fn fetch_readings(
        &self,
        requests: &[StreamFetchRequest],
    ) -> Result<Vec<StreamReadings>, BackendError> {
        self.fetch_calls.fetch_add(1, Ordering::SeqCst);
        self.fetch_requests.lock().unwrap().push(requests.to_vec());
        if self.fail_fetch {
            return Err("source unavailable".into());
        }
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        Ok(requests
            .iter()
            .map(|r| {
                let n = self.readings_per_stream;
                let mut sr = StreamReadings::new(
                    r.stream_id,
                    r.source_key.clone(),
                    (0..n)
                        .map(|i| {
                            IngestReading::new(base + chrono::Duration::seconds(i as i64), i as f64)
                        })
                        .collect(),
                );
                if self.windowed {
                    sr.window = Some(SourceWindow {
                        from: base,
                        to: base + chrono::Duration::seconds(n as i64),
                        source_rows_read: n as u64,
                        dropped_times: Vec::new(),
                        content_digest: None,
                    });
                }
                sr
            })
            .collect())
    }
}

struct Harness {
    server: MockServer,
    driver: SyncDriver,
}

async fn harness(backend: FakeBackend, streams: Vec<serde_json::Value>) -> Harness {
    let server = MockServer::start().await;
    let total = streams.len();

    // The register response carries the pinned mapping the API authors for a
    // replicate family; the driver must hand it to the backend.
    let mut register_body = stream_json(Uuid::new_v4(), "s1", None);
    register_body["replicates"] = json!([
        {"column": "m1", "index": 0},
        {"column": "m2", "index": 5, "retired": true},
    ]);
    Mock::given(method("POST"))
        .and(path("/api/streams/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(register_body))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/data_streams"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "content-range",
                    format!("data_streams 0-{total}/{total}").as_str(),
                )
                .set_body_json(&streams),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().unwrap();
            let n = body["readings"].as_array().map(|a| a.len()).unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(json!({"inserted": n}))
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest/status_events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"inserted": 1})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/actions/refresh_aggregates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let api = RiverDataClient::new(&server.uri(), "tok").unwrap();
    let driver = SyncDriver::new(Box::new(backend), api, &test_config());
    Harness { server, driver }
}

async fn count(server: &MockServer, m: &str, p: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == m && r.url.path() == p)
        .count()
}

#[tokio::test]
async fn test_discovery_runs_once_then_again_on_full() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 1,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    h.driver.sync(false).await.unwrap();
    assert_eq!(count(&h.server, "POST", "/api/streams/register").await, 1);

    h.driver.sync(true).await.unwrap();
    assert_eq!(count(&h.server, "POST", "/api/streams/register").await, 2);
}

#[tokio::test]
async fn test_ingest_batches_at_1000() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 2500,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    let result = h.driver.sync(false).await.unwrap();
    assert_eq!(result.readings_synced, 2500);

    let sizes: Vec<usize> = h
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/ingest")
        .map(|r| {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            body["readings"].as_array().unwrap().len()
        })
        .collect();
    assert_eq!(sizes, vec![1000, 1000, 500]);
}

#[tokio::test]
async fn test_since_cursor_passthrough() {
    let fetch_requests = Arc::new(Mutex::new(Vec::new()));
    let backend = FakeBackend {
        fetch_requests: fetch_requests.clone(),
        ..Default::default()
    };
    let id = Uuid::new_v4();
    let h = harness(
        backend,
        vec![stream_json(id, "s1", Some("2026-01-15T12:00:00Z"))],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    h.driver.sync(true).await.unwrap();

    let calls = fetch_requests.lock().unwrap();
    let incremental = &calls[0][0];
    assert_eq!(incremental.stream_id, id);
    assert_eq!(
        incremental.since,
        Some(Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap())
    );
    let full = &calls[1][0];
    assert_eq!(full.since, None);
}

#[tokio::test]
async fn test_retry_exhaustion_fails_sync() {
    let fetch_calls = Arc::new(AtomicU32::new(0));
    let backend = FakeBackend {
        fail_fetch: true,
        fetch_calls: fetch_calls.clone(),
        ..Default::default()
    };
    let h = harness(backend, vec![stream_json(Uuid::new_v4(), "s1", None)]).await;

    let err = h.driver.sync(false).await.unwrap_err();
    assert!(err.to_string().contains("source unavailable"));
    // RETRY_MAX counts retries after the first attempt: 1 + 3 = 4 calls.
    assert_eq!(fetch_calls.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn test_ingest_stops_at_first_failed_batch() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 2500,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    // Second ingest call fails; later batches must not be sent, or the
    // server-side cursor would advance past the gap.
    h.server.reset().await;
    let calls = Arc::new(AtomicU32::new(0));
    let calls_in_mock = calls.clone();
    Mock::given(method("GET"))
        .and(path("/api/data_streams"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-range", "data_streams 0-1/1")
                .set_body_json(vec![stream_json(Uuid::new_v4(), "s1", None)]),
        )
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest"))
        .respond_with(move |req: &Request| {
            let call = calls_in_mock.fetch_add(1, Ordering::SeqCst);
            if call == 1 {
                return ResponseTemplate::new(500);
            }
            let body: serde_json::Value = req.body_json().unwrap();
            let n = body["readings"].as_array().map(|a| a.len()).unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(json!({"inserted": n}))
        })
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/actions/refresh_aggregates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&h.server)
        .await;

    let result = h.driver.sync(false).await.unwrap();
    assert_eq!(result.readings_synced, 1000);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("1500 readings deferred"))
    );
}

#[tokio::test]
async fn test_rediscovery_skips_unchanged_descriptors() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 1,
            rediscover: true,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    h.driver.sync(false).await.unwrap();
    assert_eq!(count(&h.server, "POST", "/api/streams/register").await, 1);
}

#[tokio::test]
async fn test_full_sync_reregisters_unchanged_descriptors() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 1,
            rediscover: true,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    h.driver.sync(true).await.unwrap();
    assert_eq!(count(&h.server, "POST", "/api/streams/register").await, 2);
}

#[tokio::test]
async fn test_discovery_retried_after_failed_registration() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 1,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/streams/register"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/data_streams"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-range", "data_streams 0-1/1")
                .set_body_json(vec![stream_json(Uuid::new_v4(), "s1", None)]),
        )
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().unwrap();
            let n = body["readings"].as_array().map(|a| a.len()).unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(json!({"inserted": n}))
        })
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/actions/refresh_aggregates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&h.server)
        .await;

    // Registration fails on both cycles; the failed pass must not latch the
    // discovery flag, so the second cycle tries again.
    h.driver.sync(false).await.unwrap();
    h.driver.sync(false).await.unwrap();
    assert_eq!(count(&h.server, "POST", "/api/streams/register").await, 2);
}

#[tokio::test]
async fn test_aggregates_skipped_when_no_readings() {
    let h = harness(
        FakeBackend::default(),
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    assert_eq!(
        count(&h.server, "POST", "/api/actions/refresh_aggregates").await,
        0
    );
}

#[tokio::test]
async fn test_aggregates_refreshed_after_readings() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 5,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.driver.sync(false).await.unwrap();
    assert_eq!(
        count(&h.server, "POST", "/api/actions/refresh_aggregates").await,
        1
    );
}

// Scenario: registration returns the pinned mapping, and a listed stream's
// metadata persists one from an earlier registration. Expected behaviour: the
// backend receives both before fetch, keyed by source_key, with retired
// entries intact.
#[tokio::test]
async fn test_replicate_assignments_reach_the_backend() {
    let applied = Arc::new(Mutex::new(Vec::new()));
    let backend = FakeBackend {
        applied_assignments: applied.clone(),
        ..Default::default()
    };
    let mut listed = stream_json(Uuid::new_v4(), "STA:DOC_avg:reps", None);
    listed["metadata"] = json!({
        "replicates": {
            "source_columns": ["DOC_rep_1"],
            "assignments": [{"column": "DOC_rep_1", "index": 2}],
        }
    });
    let h = harness(backend, vec![listed]).await;

    h.driver.sync(false).await.unwrap();

    let applied = applied.lock().unwrap();
    let from_register = applied
        .iter()
        .find(|(k, _)| k == "s1")
        .expect("register response mapping applied");
    assert_eq!(from_register.1.len(), 2);
    assert_eq!(from_register.1[1].column, "m2");
    assert_eq!(from_register.1[1].index, 5);
    assert!(from_register.1[1].retired);

    let from_metadata = applied
        .iter()
        .find(|(k, _)| k == "STA:DOC_avg:reps")
        .expect("persisted metadata mapping applied");
    assert_eq!(from_metadata.1.len(), 1);
    assert_eq!(from_metadata.1[0].index, 2);
    assert!(!from_metadata.1[0].retired);
}

// A reconciled backend re-reads its source's full content every cycle. Even on an
// incremental sync of a stream carrying a cursor, the fetch must ask for the whole
// window (since: None); applying the cursor would truncate the payload and degrade
// the source silently to append mode.
#[tokio::test]
async fn test_reconciled_backend_ignores_cursor() {
    let fetch_requests = Arc::new(Mutex::new(Vec::new()));
    let backend = FakeBackend {
        readings_per_stream: 1,
        reconciled: true,
        fetch_requests: fetch_requests.clone(),
        ..Default::default()
    };
    let id = Uuid::new_v4();
    let h = harness(
        backend,
        vec![stream_json(id, "s1", Some("2026-01-15T12:00:00Z"))],
    )
    .await;

    h.driver.sync(false).await.unwrap();

    let calls = fetch_requests.lock().unwrap();
    let req = &calls[0][0];
    assert_eq!(req.stream_id, id);
    assert_eq!(req.since, None);
}

// A payload carrying a completeness claim must go out as exactly one request even
// when it exceeds the batch size: each chunk would otherwise claim the whole window
// while carrying a fraction of it, and the server would withdraw the rest.
#[tokio::test]
async fn test_windowed_payload_sent_as_single_request() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 1500,
            windowed: true,
            reconciled: true,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    // The default harness /ingest does not echo accepted_window; a windowed request
    // needs the echo or the client rejects it. Remount one that echoes the claim.
    h.server.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/data_streams"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-range", "data_streams 0-1/1")
                .set_body_json(vec![stream_json(Uuid::new_v4(), "s1", None)]),
        )
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().unwrap();
            let n = body["readings"].as_array().map(|a| a.len()).unwrap_or(0);
            ResponseTemplate::new(200)
                .set_body_json(json!({"inserted": n, "accepted_window": body["window"]}))
        })
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/actions/refresh_aggregates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&h.server)
        .await;

    let result = h.driver.sync(false).await.unwrap();
    assert_eq!(result.readings_synced, 1500);
    assert_eq!(count(&h.server, "POST", "/api/ingest").await, 1);
}

// A windowed request whose response omits accepted_window is an older API silently
// ignoring the claim. The client must reject it rather than downgrade the source to
// append mode with no record.
#[tokio::test]
async fn test_windowed_payload_errors_without_accepted_window_echo() {
    let h = harness(
        FakeBackend {
            readings_per_stream: 10,
            windowed: true,
            reconciled: true,
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    let result = h.driver.sync(false).await.unwrap();
    assert_eq!(result.readings_synced, 0);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("did not echo the completeness window"))
    );
}

// Scenario: the API reports a non-zero `held` on an ingest (a hypothetical image that still
// counts audit holds). Expected behaviour: the count is carried into the result and nothing
// else changes. No re-fetch, no second ingest, no error: the audit admits every group and the
// cursor advances regardless (ADR 0002), so `held` is never a resend signal.
#[tokio::test]
async fn test_a_non_zero_held_count_is_reported_and_not_acted_on() {
    let fetch_calls = Arc::new(AtomicU32::new(0));
    let h = harness(
        FakeBackend {
            readings_per_stream: 6,
            fetch_calls: fetch_calls.clone(),
            ..Default::default()
        },
        vec![stream_json(Uuid::new_v4(), "s1", None)],
    )
    .await;

    h.server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/streams/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(stream_json(
            Uuid::new_v4(),
            "s1",
            None,
        )))
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/data_streams"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-range", "data_streams 0-1/1")
                .set_body_json(vec![stream_json(Uuid::new_v4(), "s1", None)]),
        )
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/ingest"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value = req.body_json().unwrap();
            let n = body["readings"].as_array().map(|a| a.len()).unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(json!({"inserted": n, "held": 3}))
        })
        .mount(&h.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/actions/refresh_aggregates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&h.server)
        .await;

    let result = h.driver.sync(false).await.unwrap();
    assert_eq!(result.readings_synced, 6);
    assert_eq!(result.readings_held, 3, "reported as received");
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(fetch_calls.load(Ordering::SeqCst), 1, "no re-fetch");
    assert_eq!(
        count(&h.server, "POST", "/api/ingest").await,
        1,
        "no re-send"
    );
}
