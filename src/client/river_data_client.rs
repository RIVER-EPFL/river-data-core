use reqwest::Client;
use std::time::Duration;
use uuid::Uuid;

use crate::error::RiverDataClientError;
use crate::models::{
    AnnotationMapping, AnnotationUpsert, CommandStatus, CurveMapping, DataStream, GroupAudit,
    IngestReading, IngestStatusEvent, NoteMapping, NoteUpsert, RegisterStreamRequest, SensorUpsert,
    StandardCurveUpsert, SyncEventCreate, SyncEventRef, SyncEventUpdate,
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
    /// Windowed diff: stored keys the source has moved since river-data stored them. Nothing is
    /// written for them; `proposed` says how many are waiting for a person to accept or reject.
    pub changed: u64,
    /// Windowed diff: changed keys recorded as proposals this pass, each awaiting a decision.
    /// Absent on an API older than the proposal queue, which reads as 0.
    pub proposed: u64,
    /// Windowed diff: stored rows absent from the claimed window, stamped withdrawn.
    pub withdrawn: u64,
    /// Windowed diff: stored rows the payload re-sent unchanged (proof the pass looked).
    pub unchanged: u64,
}

/// Clip a server message to `max` characters on a character boundary, marking that it was clipped.
fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &text[..idx]),
        None => text.to_string(),
    }
}

/// Outcome of a chunked ingest.
#[derive(Debug, Default)]
pub struct BatchedIngest {
    pub inserted: u64,
    pub skipped: u64,
    pub skipped_reasons: Vec<String>,
    pub changed: u64,
    pub proposed: u64,
    pub withdrawn: u64,
    pub unchanged: u64,
    pub failed_batches: usize,
    /// Readings not attempted because an earlier batch failed.
    pub deferred: usize,
    /// Why each failed batch failed, as the server explained it. The ledger row is the operator's
    /// only view of a cycle, so a refusal that is not carried here is a refusal nobody can read.
    pub errors: Vec<String>,
}

/// Per-request ingest flags and audit payload.
#[derive(Debug, Default, Clone, Copy)]
pub struct IngestOptions<'a> {
    /// Update existing rows in place (sync services only).
    pub overwrite: bool,
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
            .send_authorized(
                self.http_client.post(self.url("/streams/register")).json(req),
                "register_stream",
            )
            .await?;
        let resp = self.check_response(resp).await?;
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
                .send_authorized(
                    self.http_client.get(self.url("/data_streams")).query(&[
                        ("filter", filter_str.as_str()),
                        ("range", range_str.as_str()),
                        ("sort", r#"["id","ASC"]"#),
                    ]),
                    "list_streams",
                )
                .await?;
            let resp = self.check_response(resp).await?;

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
            // Windowed diff counts; absent on an API older than reconciliation.
            #[serde(default)]
            changed: u64,
            #[serde(default)]
            proposed: u64,
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
        if !opts.audits.is_empty() {
            body["audit"] = serde_json::to_value(opts.audits)
                .map_err(|e| RiverDataClientError::Api(format!("serialize audits: {e}")))?;
        }
        if let Some(window) = opts.window {
            body["window"] = serde_json::to_value(window)
                .map_err(|e| RiverDataClientError::Api(format!("serialize window: {e}")))?;
        }
        let resp = self
            .send_authorized(
                self.http_client.post(self.url("/ingest")).json(&body),
                "ingest_readings",
            )
            .await?;
        let resp = self.check_response(resp).await?;
        let result: IngestResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse ingest response: {e}")))?;
        if opts.window.is_some() && result.accepted_window.is_none() {
            return Err(RiverDataClientError::Api(
                "the API did not echo the completeness window; it is running an image without windowed reconciliation and the claim was silently ignored"
                    .to_string(),
            ));
        }
        Ok(IngestOutcome {
            inserted: result.inserted,
            skipped: result.skipped,
            skipped_reasons: result.skipped_reasons,
            changed: result.changed,
            proposed: result.proposed,
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
            .send_authorized(
                self.http_client.post(self.url("/ingest/status_events")).json(&body),
                "ingest_status_events",
            )
            .await?;
        let resp = self.check_response(resp).await?;
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
                    result.changed += outcome.changed;
                    result.proposed += outcome.proposed;
                    result.withdrawn += outcome.withdrawn;
                    result.unchanged += outcome.unchanged;
                }
                Err(e) => {
                    tracing::warn!(%stream_id, batch_len = ordered.len(), error = %e, "Windowed ingest failed; the window will be re-asserted next cycle");
                    result.failed_batches += 1;
                    result.deferred = readings.len();
                    result.errors.push(e.to_string());
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
                    result.changed += outcome.changed;
                    result.proposed += outcome.proposed;
                    result.withdrawn += outcome.withdrawn;
                    result.unchanged += outcome.unchanged;
                    sent += chunk.len();
                }
                Err(e) => {
                    tracing::warn!(%stream_id, batch_len = chunk.len(), error = %e, "Ingest batch failed, deferring rest of stream to next cycle");
                    result.failed_batches += 1;
                    result.deferred = readings.len() - sent;
                    result.errors.push(e.to_string());
                    break;
                }
            }
        }
        result
    }

    // ========================================================================
    // Instruments
    // ========================================================================

    /// Offer the source's own instrument register to the API, which stores it as proposals a
    /// pairing plan admits. Nothing is minted here: an instrument exists once a plan an operator
    /// validated creates it (Q134), so a register row travels and waits.
    pub async fn propose_instruments(
        &self,
        source_system: &str,
        instruments: &[SensorUpsert],
    ) -> Result<usize, RiverDataClientError> {
        if instruments.is_empty() {
            return Ok(0);
        }
        #[derive(serde::Deserialize)]
        struct ProposalsResponse {
            #[serde(default)]
            stored: usize,
        }
        let body = serde_json::json!({
            "source_system": source_system,
            "instruments": instruments,
        });
        let resp = self
            .send_authorized(
                self.http_client
                    .post(self.url("/sensors/proposals"))
                    .json(&body),
                "propose_instruments",
            )
            .await?;
        let resp = self.check_response(resp).await?;
        let parsed: ProposalsResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("propose instruments: {e}")))?;
        Ok(parsed.stored)
    }

    // ========================================================================
    // Standard Curves
    // ========================================================================

    /// Register portal standard curves; idempotent per (source_system,
    /// source_key). Returns the API-side identity of every curve stored; a curve
    /// held for a pairing plan has none yet and is left out.
    pub async fn register_standard_curves(
        &self,
        source_system: &str,
        curves: &[StandardCurveUpsert],
    ) -> Result<Vec<CurveMapping>, RiverDataClientError> {
        let mut mappings = Vec::with_capacity(curves.len());
        for curve in curves {
            let mut body = serde_json::to_value(curve)
                .map_err(|e| RiverDataClientError::Api(format!("serialize curve: {e}")))?;
            body["source_system"] = serde_json::Value::String(source_system.to_string());
            let resp = self
                .send_authorized(
                    self.http_client
                        .post(self.url("/standard_curves/register"))
                        .json(&body),
                    "register_standard_curve",
                )
                .await?;
            let resp = self.check_response(resp).await?;
            let parsed: CurveResponse = resp
                .json()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("parse curve response: {e}")))?;
            mappings.extend(curve_mapping(&curve.source_key, parsed));
        }
        Ok(mappings)
    }

    /// The `(source_system, source_key)` set of lab curves this source has replicated here, for
    /// the source audit. `Ok(None)` on an API that does not serve the collection, which is the
    /// difference between "the store holds none" and "the store cannot be asked": reporting every
    /// source curve as missing because the listing 404'd would be a false finding.
    pub async fn list_standard_curve_keys(
        &self,
        source_system: &str,
    ) -> Result<Option<Vec<String>>, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct CurveRow {
            source_key: Option<String>,
        }

        let filter = serde_json::json!({ "source_system": source_system }).to_string();
        let resp = self
            .send_authorized(
                self.http_client.get(self.url("/standard_curves")).query(&[
                    ("filter", filter.as_str()),
                    ("range", "[0,9999]"),
                    ("sort", r#"["id","ASC"]"#),
                ]),
                "list_standard_curves",
            )
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = self.check_response(resp).await?;
        let rows: Vec<CurveRow> = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse standard curves: {e}")))?;
        Ok(Some(rows.into_iter().filter_map(|r| r.source_key).collect()))
    }

    // ========================================================================
    // Annotations
    // ========================================================================

    /// Register source-authored annotations; idempotent per (source_system,
    /// source_key), so re-asserting a key updates in place. One batched
    /// request; the API resolves site and parameter from each stream's
    /// pairing and reports `unpaired` for streams that have none yet.
    pub async fn register_annotations(
        &self,
        source_system: &str,
        annotations: &[AnnotationUpsert],
    ) -> Result<Vec<AnnotationMapping>, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct RegisterResponse {
            annotations: Vec<AnnotationMapping>,
        }

        let body = serde_json::json!({
            "source_system": source_system,
            "annotations": annotations,
        });
        let resp = self
            .send_authorized(
                self.http_client.post(self.url("/annotations/register")).json(&body),
                "register_annotations",
            )
            .await?;
        let resp = self.check_response(resp).await?;
        let parsed: RegisterResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse annotations response: {e}")))?;
        Ok(parsed.annotations)
    }

    // ========================================================================
    // Site notes
    // ========================================================================

    /// Register a source's site notes; idempotent per (source_system,
    /// source_key). Returns one outcome per note, including the ones whose
    /// station resolved to no site.
    pub async fn register_notes(
        &self,
        source_system: &str,
        notes: &[NoteUpsert],
    ) -> Result<Vec<NoteMapping>, RiverDataClientError> {
        #[derive(serde::Deserialize)]
        struct RegisterResponse {
            notes: Vec<NoteMapping>,
        }

        let body = serde_json::json!({
            "source_system": source_system,
            "notes": notes,
        });
        let resp = self
            .send_authorized(
                self.http_client.post(self.url("/notes/register")).json(&body),
                "register_notes",
            )
            .await?;
        let resp = self.check_response(resp).await?;
        let parsed: RegisterResponse = resp
            .json()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("parse notes response: {e}")))?;
        Ok(parsed.notes)
    }

    // ========================================================================
    // Actions
    // ========================================================================

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
            .send_authorized(
                self.http_client
                    .patch(self.url(&format!("/sync/commands/{command_id}")))
                    .json(&body),
                "update_command",
            )
            .await?;
        self.check_response(resp).await?;
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
                .send_authorized(
                    self.http_client.post(self.url("/sync/events")).json(event),
                    "create_sync_event",
                )
                .await;
            match resp {
                Ok(resp) => match self.check_response(resp).await {
                    Ok(resp) => {
                        return resp.json().await.map_err(|e| {
                            RiverDataClientError::Api(format!("parse sync_event: {e}"))
                        });
                    }
                    Err(e) => last_err = Some(e),
                },
                Err(e) => last_err = Some(e),
            }
            if attempt < 2 {
                tracing::warn!(attempt, "create_sync_event refused; retrying");
            }
        }
        Err(last_err.expect("at least one attempt ran"))
    }

    pub async fn update_sync_event(
        &self,
        event_id: Uuid,
        update: &SyncEventUpdate,
    ) -> Result<(), RiverDataClientError> {
        let resp = self
            .send_authorized(
                self.http_client
                    .patch(self.url(&format!("/sync/events/{event_id}")))
                    .json(update),
                "update_sync_event",
            )
            .await?;
        self.check_response(resp).await?;
        Ok(())
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Send with the current session token; on 401, re-send once with the token
    /// as it stands now. The heartbeat rotates the session token, so a request
    /// in flight across a rotation carries a token that was just retired.
    async fn send_authorized(
        &self,
        req: reqwest::RequestBuilder,
        what: &str,
    ) -> Result<reqwest::Response, RiverDataClientError> {
        let retry = req.try_clone();
        let resp = req
            .bearer_auth(self.current_token())
            .send()
            .await
            .map_err(|e| RiverDataClientError::Api(format!("{what} failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED
            && let Some(retry) = retry
        {
            return retry
                .bearer_auth(self.current_token())
                .send()
                .await
                .map_err(|e| RiverDataClientError::Api(format!("{what} failed: {e}")));
        }
        Ok(resp)
    }

    /// Fail on a non-2xx, carrying the server's own explanation. The reason a request was refused
    /// lives only in the body (a dishonest completeness window, a window on a non-spot stream, a
    /// project-scope rejection), and the error text is what reaches `sync_events.errors`, so a bare
    /// status line sends an operator to the pod logs to learn anything at all.
    async fn check_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<reqwest::Response, RiverDataClientError> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let url = resp.url().clone();
        let body = resp.text().await.unwrap_or_default();
        let body = body.trim();
        Err(RiverDataClientError::Api(if body.is_empty() {
            format!("HTTP {status} from {url}")
        } else {
            // An error page rather than an API message would otherwise fill the ledger row.
            format!("HTTP {status} from {url}: {}", truncate(body, 500))
        }))
    }
}

/// The API's answer to one curve registration. A curve no pairing plan has created yet is held,
/// and carries no id and no instrument.
#[derive(serde::Deserialize)]
struct CurveResponse {
    id: Option<Uuid>,
    sensor_id: Option<Uuid>,
    #[serde(default)]
    superseded: bool,
}

/// The mapping a registration yields: a stored curve maps its source key to its id and instrument,
/// a held one to nothing.
fn curve_mapping(source_key: &str, response: CurveResponse) -> Option<CurveMapping> {
    Some(CurveMapping {
        source_key: source_key.to_string(),
        id: response.id?,
        sensor_id: response.sensor_id?,
        superseded: response.superseded,
    })
}

#[cfg(test)]
#[path = "tests/river_data_client.rs"]
mod tests;
