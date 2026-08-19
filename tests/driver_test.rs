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
use river_data_core::models::{IngestReading, RunnerConfig};

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

#[derive(Default)]
struct FakeBackend {
    readings_per_stream: usize,
    fail_fetch: bool,
    rediscover: bool,
    fetch_calls: Arc<AtomicU32>,
    fetch_requests: Arc<Mutex<Vec<Vec<StreamFetchRequest>>>>,
}

#[async_trait::async_trait]
impl SourceBackend for FakeBackend {
    fn source_system(&self) -> &str {
        "fake"
    }

    fn rediscover_every_cycle(&self) -> bool {
        self.rediscover
    }

    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
        Ok(vec![StreamDescriptor {
            source_key: "s1".to_string(),
            source_name: "Stream 1".to_string(),
            source_path: "fake/s1".to_string(),
            metadata: json!({}),
            measurement_type: Some("continuous".to_string()),
            sensor_id: None,
        }])
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
            .map(|r| StreamReadings {
                stream_id: r.stream_id,
                source_key: r.source_key.clone(),
                readings: (0..self.readings_per_stream)
                    .map(|i| {
                        IngestReading::new(base + chrono::Duration::seconds(i as i64), i as f64)
                    })
                    .collect(),
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

    Mock::given(method("POST"))
        .and(path("/api/streams/register"))
        .respond_with(ResponseTemplate::new(200).set_body_json(stream_json(
            Uuid::new_v4(),
            "s1",
            None,
        )))
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
async fn test_rediscovery_every_cycle_when_backend_asks() {
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
