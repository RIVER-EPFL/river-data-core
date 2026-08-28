use reqwest::Client;
use std::time::Duration;
use uuid::Uuid;

use crate::error::RiverDataClientError;
use crate::models::{
    CommandStatus, CurveMapping, DataStream, GroupAudit, IngestReading, IngestStatusEvent,
    RegisterStreamRequest, StandardCurveUpsert, SyncEventCreate, SyncEventRef, SyncEventUpdate,
};

pub struct RiverDataClient {
    http_client: Client,
    base_url: String,
    path_prefix: String,
    token: std::sync::RwLock<String>,
}

/// Outcome of a single ingest call.
#[derive(Debug, Default)]
pub struct IngestOutcome {
    pub inserted: u64,
    /// Readings the API refused admission (out of window, non-finite, unknown
    /// measurement type). They are dropped, not deferred: the stream cursor
    /// advances past them.
    pub skipped: u64,
    /// One entry per rejection kind, with its count.
    pub skipped_reasons: Vec<String>,
    /// Readings withheld pending audit acknowledgement. The API caps the
    /// stream cursor below the earliest held group, so the next incremental
    /// fetch re-sends them; no client-side handling beyond reporting.
    pub held: u64,
    /// Windowed diff: stored rows whose source value changed and were corrected in place.
    pub changed: u64,
    /// Windowed diff: stored rows absent from the claimed window, stamped withdrawn.
    pub withdrawn: u64,
    /// Windowed diff: stored rows the payload re-sent unchanged (proof the pass looked).
    pub unchanged: u64,
}

/// Outcome of a chunked ingest.
#[derive(Debug, Default)]
pub struct BatchedIngest {
    pub inserted: u64,
    pub skipped: u64,
    pub skipped_reasons: Vec<String>,
    pub held: u64,
    pub changed: u64,
    pub withdrawn: u64,
    pub unchanged: u64,
    pub failed_batches: usize,
    /// Readings not attempted because an earlier batch failed.
    pub deferred: usize,
}

/// Per-request ingest flags and audit payload.
#[derive(Debug, Default, Clone, Copy)]
pub struct IngestOptions<'a> {
    /// Update existing rows in place (sync services only).
    pub overwrite: bool,
    /// Mark the readings as replicate collections.
    pub collection: bool,
    /// Group audits; each request carries only the entries whose time falls
    /// inside that chunk.
    pub audits: &'a [GroupAudit],
    /// Completeness claim: the payload is the source's complete content over the window. The
    /// server diffs and converges; a request carrying one is never chunked, because each chunk
    /// would claim the whole window with a partial payload.
    pub window: Option<&'a crate::models::SourceWindow>,
}

/// Split readings into chunks of at most `batch_size` rows without splitting a
/// run of identical timestamps across requests. `readings` must be sorted by
/// time; a replicate group split mid-request would be audited (and held)
/// against half its members, stranding the rest behind the cursor.
/// A single run larger than `batch_size` becomes one oversized chunk.
fn group_safe_chunks(readings: &[IngestReading], batch_size: usize) -> Vec<&[IngestReading]> {
    let batch_size = batch_size.max(1);
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < readings.len() {
        let mut end = (start + batch_size).min(readings.len());
        if end < readings.len() {
            let boundary_time = readings[end - 1].time;
            if readings[end].time == boundary_time {
                // Grow to cover the whole run when the run spans the cut.
                while end < readings.len() && readings[end].time == boundary_time {
                    end += 1;
                }
                // Prefer cutting before the run when that leaves a non-empty chunk.
                let mut run_start = end;
                while run_start > start && readings[run_start - 1].time == boundary_time {
                    run_start -= 1;
                }
                if run_start > start && end - start > batch_size {
                    end = run_start;
                }
            }
        }
        chunks.push(&readings[start..end]);
        start = end;
    }
    chunks
}

impl RiverDataClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self, reqwest::Error> {
        Self::with_config(base_url, token, "/api", 60)
    }

    pub fn with_config(
        base_url: &str,
        token: &str,
        path_prefix: &str,
        timeout_secs: u64,
    ) -> Result<Self, reqwest::Error> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;

        Ok(Self {
            http_client,
            base_url: base_url.trim_end_matches('/').to_string(),
            path_prefix: path_prefix.to_string(),
            token: std::sync::RwLock::new(token.to_string()),
        })
    }

    pub fn set_token(&self, token: &str) {
        if let Ok(mut t) = self.token.write() {
            *t = token.to_string();
        }
    }

    fn current_token(&self) -> String {
        self.token.read().map(|t| t.clone()).unwrap_or_default()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base_url, self.path_prefix, path)
    }

    // ========================================================================
    // Stream Registration
    // ========================================================================

    pub async fn register_stream(
        &self,
        req: &RegisterStreamRequest,
    ) -> Result<DataStream, RiverDataClientError> {
        let resp = self
            .http_client
            .post(self.url("/streams/register"))
            .bearer_auth(self.current_token())
            .json(req)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("register_stream failed: {e}")))?;
        self.check_response(&resp)?;
        resp.json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse stream: {e}")))
    }

    pub async fn list_streams(
        &self,
        source_system: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<DataStream>, RiverDataClientError> {
        const PAGE_SIZE: usize = 1000;
        let mut all_items: Vec<DataStream> = Vec::new();
        let mut offset: usize = 0;

        let mut filter = serde_json::Map::new();
        if let Some(ss) = source_system {
            filter.insert(
                "source_system".into(),
                serde_json::Value::String(ss.to_string()),
            );
        }
        if let Some(active) = is_active {
            filter.insert("is_active".into(), serde_json::Value::Bool(active));
        }
        let filter_str = serde_json::Value::Object(filter).to_string();

        loop {
            let end = offset + PAGE_SIZE - 1;
            let range_str = format!("[{offset},{end}]");

            let resp = self
                .http_client
                .get(self.url("/data_streams"))
                .query(&[
                    ("filter", filter_str.as_str()),
                    ("range", range_str.as_str()),
                    ("sort", r#"["id","ASC"]"#),
                ])
                .bearer_auth(self.current_token())
                .send()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("list_streams failed: {e}")))?;
            self.check_response(&resp)?;

            let total = Self::parse_content_range_total(&resp);

            let page: Vec<DataStream> = resp
                .json()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("parse streams: {e}")))?;

            let page_len = page.len();
            all_items.extend(page);

            match total {
                Some(t) if all_items.len() >= t => break,
                None => break,
                _ => {}
            }
            if page_len < PAGE_SIZE {
                break;
            }
            offset += PAGE_SIZE;
        }

        Ok(all_items)
    }

    fn parse_content_range_total(resp: &reqwest::Response) -> Option<usize> {
        let header = resp.headers().get("content-range")?.to_str().ok()?;
        let total_str = header.rsplit('/').next()?;
        total_str.parse().ok()
    }

    // ========================================================================
    // Data Ingestion
    // ========================================================================

    pub async fn ingest_readings(
        &self,
        stream_id: Uuid,
        readings: &[IngestReading],
    ) -> Result<IngestOutcome, RiverDataClientError> {
        self.ingest_readings_with(stream_id, readings, IngestOptions::default())
            .await
    }

    pub async fn ingest_readings_with(
        &self,
        stream_id: Uuid,
        readings: &[IngestReading],
        opts: IngestOptions<'_>,
    ) -> Result<IngestOutcome, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct IngestResponse {
            inserted: u64,
            // Absent on an API older than the per-reading admission change.
            #[serde(default)]
            skipped: u64,
            #[serde(default)]
            skipped_reasons: Vec<String>,
            // Absent on an API older than replicate audits.
            #[serde(default)]
            held: u64,
            // Windowed diff counts; absent on an API older than reconciliation.
            #[serde(default)]
            changed: u64,
            #[serde(default)]
            withdrawn: u64,
            #[serde(default)]
            unchanged: u64,
            // The window the server accepted, echoed back. A missing echo on a request that
            // carried a window means the API silently ignored the claim (an older image), and
            // treating that as success would downgrade the source to append mode with no record.
            #[serde(default)]
            accepted_window: Option<serde_json::Value>,
        }

        let mut body = serde_json::json!({
            "stream_id": stream_id,
            "readings": readings,
        });
        if opts.overwrite {
            body["overwrite"] = serde_json::Value::Bool(true);
        }
        if opts.collection {
            body["collection"] = serde_json::Value::Bool(true);
        }
        if !opts.audits.is_empty() {
            body["audit"] = serde_json::to_value(opts.audits)
                .map_err(|e| RiverDataClientError::Api(format!("serialize audits: {e}")))?;
        }
        if let Some(window) = opts.window {
            body["window"] = serde_json::to_value(window)
                .map_err(|e| RiverDataClientError::Api(format!("serialize window: {e}")))?;
        }
        let resp = self
            .http_client
            .post(self.url("/ingest"))
            .bearer_auth(self.current_token())
            .json(&body)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("ingest_readings failed: {e}")))?;
        self.check_response(&resp)?;
        let result: IngestResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse ingest response: {e}")))?;
        if opts.window.is_some() && result.accepted_window.is_none() {
            return Err(RiverDataClientError::Api(
                "the API did not echo the completeness window; it is running an image without                  windowed reconciliation and the claim was silently ignored"
                    .to_string(),
            ));
        }
        Ok(IngestOutcome {
            inserted: result.inserted,
            skipped: result.skipped,
            skipped_reasons: result.skipped_reasons,
            held: result.held,
            changed: result.changed,
            withdrawn: result.withdrawn,
            unchanged: result.unchanged,
        })
    }

    pub async fn ingest_status_events(
        &self,
        stream_id: Uuid,
        events: &[IngestStatusEvent],
    ) -> Result<u64, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct IngestResponse {
            inserted: u64,
        }

        let body = serde_json::json!({
            "stream_id": stream_id,
            "events": events,
        });
        let resp = self
            .http_client
            .post(self.url("/ingest/status_events"))
            .bearer_auth(self.current_token())
            .json(&body)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("ingest_status_events failed: {e}")))?;
        self.check_response(&resp)?;
        let result: IngestResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse ingest response: {e}")))?;
        Ok(result.inserted)
    }

    /// Chunked ingest. Stops at the first failed batch: chunks are sent
    /// time-ascending, and a later successful batch would advance the server's
    /// stream cursor past the failed window, turning it into a permanent gap.
    /// Stopping leaves the cursor at the last contiguous point so the next
    /// cycle re-fetches the remainder.
    pub async fn ingest_readings_batched(
        &self,
        stream_id: Uuid,
        readings: &[IngestReading],
        batch_size: usize,
    ) -> BatchedIngest {
        self.ingest_readings_batched_with(stream_id, readings, batch_size, IngestOptions::default())
            .await
    }

    pub async fn ingest_readings_batched_with(
        &self,
        stream_id: Uuid,
        readings: &[IngestReading],
        batch_size: usize,
        opts: IngestOptions<'_>,
    ) -> BatchedIngest {
        // The server cursor is forward-only and moves to the newest reading it
        // accepted, so a chunk out of time order can carry the cursor past rows
        // a later chunk still has to send. Sorting here makes the ascending
        // order the contract depends on hold for every backend. The secondary
        // replicate_index key keeps a group's members in index order within a
        // request.
        let mut ordered = readings.to_vec();
        ordered.sort_by_key(|r| (r.time, r.replicate_index));

        let mut result = BatchedIngest::default();

        // A completeness claim covers the whole payload, so it goes out as one request: each
        // chunk would otherwise claim the full window while carrying a fraction of it, and the
        // server would withdraw the rest.
        if opts.window.is_some() {
            match self.ingest_readings_with(stream_id, &ordered, opts).await {
                Ok(outcome) => {
                    result.inserted += outcome.inserted;
                    result.skipped += outcome.skipped;
                    result.skipped_reasons.extend(outcome.skipped_reasons);
                    result.held += outcome.held;
                    result.changed += outcome.changed;
                    result.withdrawn += outcome.withdrawn;
                    result.unchanged += outcome.unchanged;
                }
                Err(e) => {
                    tracing::warn!(%stream_id, batch_len = ordered.len(), error = %e, "Windowed ingest failed; the window will be re-asserted next cycle");
                    result.failed_batches += 1;
                    result.deferred = readings.len();
                }
            }
            return result;
        }
        let mut sent = 0usize;
        for chunk in group_safe_chunks(&ordered, batch_size) {
            // Only the audits for groups in this chunk; group-safe chunking
            // guarantees a group's time falls in exactly one chunk.
            let (first, last) = (chunk[0].time, chunk[chunk.len() - 1].time);
            let chunk_audits: Vec<GroupAudit> = opts
                .audits
                .iter()
                .filter(|a| a.time >= first && a.time <= last)
                .cloned()
                .collect();
            let chunk_opts = IngestOptions {
                overwrite: opts.overwrite,
                collection: opts.collection,
                audits: &chunk_audits,
                window: None,
            };
            match self
                .ingest_readings_with(stream_id, chunk, chunk_opts)
                .await
            {
                Ok(outcome) => {
                    result.inserted += outcome.inserted;
                    result.skipped += outcome.skipped;
                    result.skipped_reasons.extend(outcome.skipped_reasons);
                    result.held += outcome.held;
                    result.changed += outcome.changed;
                    result.withdrawn += outcome.withdrawn;
                    result.unchanged += outcome.unchanged;
                    sent += chunk.len();
                }
                Err(e) => {
                    tracing::warn!(%stream_id, batch_len = chunk.len(), error = %e, "Ingest batch failed, deferring rest of stream to next cycle");
                    result.failed_batches += 1;
                    result.deferred = readings.len() - sent;
                    break;
                }
            }
        }
        result
    }

    // ========================================================================
    // Standard Curves
    // ========================================================================

    /// Register portal standard curves; idempotent per (source_system,
    /// source_key). Returns the API-side identity of every curve registered.
    pub async fn register_standard_curves(
        &self,
        source_system: &str,
        curves: &[StandardCurveUpsert],
    ) -> Result<Vec<CurveMapping>, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct CurveResponse {
            id: Uuid,
            sensor_id: Uuid,
            #[serde(default)]
            superseded: bool,
        }

        let mut mappings = Vec::with_capacity(curves.len());
        for curve in curves {
            let mut body = serde_json::to_value(curve)
                .map_err(|e| RiverDataClientError::Api(format!("serialize curve: {e}")))?;
            body["source_system"] = serde_json::Value::String(source_system.to_string());
            let resp = self
                .http_client
                .post(self.url("/standard_curves/register"))
                .bearer_auth(self.current_token())
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    RiverDataClientError::Api(format!("register_standard_curve failed: {e}"))
                })?;
            self.check_response(&resp)?;
            let parsed: CurveResponse = resp
                .json()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("parse curve response: {e}")))?;
            mappings.push(CurveMapping {
                source_key: curve.source_key.clone(),
                id: parsed.id,
                sensor_id: parsed.sensor_id,
                superseded: parsed.superseded,
            });
        }
        Ok(mappings)
    }

    // ========================================================================
    // Actions
    // ========================================================================

    pub async fn refresh_aggregates(&self, full: bool) -> Result<(), RiverDataClientError> {
        let body = serde_json::json!({ "full": full });
        let resp = self
            .http_client
            .post(self.url("/actions/refresh_aggregates"))
            .bearer_auth(self.current_token())
            .json(&body)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("refresh_aggregates failed: {e}")))?;
        self.check_response(&resp)?;
        Ok(())
    }

    // ========================================================================
    // Command Updates
    // ========================================================================

    pub async fn update_command(
        &self,
        command_id: Uuid,
        status: CommandStatus,
        result: Option<serde_json::Value>,
    ) -> Result<(), RiverDataClientError> {
        let body = serde_json::json!({ "status": status.as_str(), "result": result });
        let resp = self
            .http_client
            .patch(self.url(&format!("/sync/commands/{command_id}")))
            .bearer_auth(self.current_token())
            .json(&body)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("update_command failed: {e}")))?;
        self.check_response(&resp)?;
        Ok(())
    }

    // ========================================================================
    // Sync Events
    // ========================================================================

    pub async fn create_sync_event(
        &self,
        event: &SyncEventCreate,
    ) -> Result<SyncEventRef, RiverDataClientError> {
        // The cycle record is the observability record: a transient refusal (a 429 during a
        // multi-service boot was observed to lose METALP's cycle record while its data synced
        // fully) must not silently drop it, so the send retries before giving up.
        let mut last_err = None;
        for attempt in 0..3u32 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(2 << attempt)).await;
            }
            let resp = self
                .http_client
                .post(self.url("/sync/events"))
                .bearer_auth(self.current_token())
                .json(event)
                .send()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("create_sync_event failed: {e}")));
            match resp {
                Ok(resp) => match self.check_response(&resp) {
                    Ok(()) => {
                        return resp.json().await.map_err(|e| {
                            RiverDataClientError::Api(format!("parse sync_event: {e}"))
                        });
                    }
                    Err(e) => last_err = Some(e),
                },
                Err(e) => last_err = Some(e),
            }
            tracing::warn!(attempt, "create_sync_event refused; retrying");
        }
        Err(last_err.expect("at least one attempt ran"))
    }

    pub async fn update_sync_event(
        &self,
        event_id: Uuid,
        update: &SyncEventUpdate,
    ) -> Result<(), RiverDataClientError> {
        let resp = self
            .http_client
            .patch(self.url(&format!("/sync/events/{event_id}")))
            .bearer_auth(self.current_token())
            .json(update)
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("update_sync_event failed: {e}")))?;
        self.check_response(&resp)?;
        Ok(())
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn check_response(&self, resp: &reqwest::Response) -> Result<(), RiverDataClientError> {
        if !resp.status().is_success() {
            return Err(RiverDataClientError::Api(format!(
                "HTTP {} from {}",
                resp.status(),
                resp.url()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
