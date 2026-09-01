use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use uuid::Uuid;

use crate::client::backend::SourceBackend;
use crate::client::river_data_client::{IngestOptions, RiverDataClient};
use crate::client::service::SyncService;
use crate::commands;
use crate::models::{
    ColumnAssignment, DataStream, RegisterStreamRequest, RunnerConfig, StreamFetchRequest,
    StreamReadings, SyncResult,
};

pub const INGEST_BATCH_SIZE: usize = 1000;

/// Sync-event log lines per cycle; further per-stream detail goes to debug logging.
const MAX_LOG_LINES: usize = 50;

/// Bound a line collection to MAX_LOG_LINES with a count of what was dropped.
fn cap_lines(lines: &mut Vec<String>) {
    if lines.len() > MAX_LOG_LINES {
        let dropped = lines.len() - MAX_LOG_LINES;
        lines.truncate(MAX_LOG_LINES);
        lines.push(format!("... and {dropped} more"));
    }
}

/// Drives a `SourceBackend` through the full sync cycle: stream registration,
/// cursor tracking, batched ingest, aggregate refresh and status events.
pub struct SyncDriver {
    backend: Box<dyn SourceBackend>,
    api: RiverDataClient,
    retry_max: u32,
    retry_delay_secs: u64,
    discovered: AtomicBool,
    /// Hash of the last successfully sent RegisterStreamRequest per source_key;
    /// an unchanged descriptor is not re-registered.
    registered: Mutex<HashMap<String, u64>>,
}

/// FNV-1a over the canonical bytes; deterministic across processes and restarts.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Digest of a windowed payload's source-asserted content: the window claim, the rows sorted by
/// (time, replicate_index), the audit expectations and the riding annotations. Server curation
/// never enters it. None for unwindowed payloads and for content that cannot serialize (which
/// then sends as before).
fn window_digest(sr: &StreamReadings) -> Option<String> {
    let w = sr.window.as_ref()?;
    let mut rows: Vec<_> = sr.readings.iter().collect();
    rows.sort_by_key(|r| (r.time, r.replicate_index));
    let mut audits: Vec<_> = sr.audits.iter().collect();
    audits.sort_by_key(|a| a.time);
    let mut annotations: Vec<_> = sr.annotations.iter().collect();
    annotations.sort_by_key(|a| (a.time, a.source_key.clone()));
    let canonical = serde_json::json!({
        "window": {
            "from": w.from,
            "to": w.to,
            "source_rows_read": w.source_rows_read,
            "dropped_times": w.dropped_times,
        },
        "rows": rows,
        "audits": audits,
        "annotations": annotations,
        "collection": sr.collection,
    });
    let bytes = serde_json::to_vec(&canonical).ok()?;
    Some(format!("{:016x}", fnv1a64(&bytes)))
}

struct ReadingsOutcome {
    streams: Vec<DataStream>,
    readings_synced: u64,
    readings_skipped: u64,
    readings_held: u64,
    streams_with_data: usize,
    log: Vec<String>,
    errors: Vec<String>,
}

impl SyncDriver {
    pub fn new(
        backend: Box<dyn SourceBackend>,
        api: RiverDataClient,
        config: &RunnerConfig,
    ) -> Self {
        Self {
            backend,
            api,
            retry_max: config.retry_max,
            retry_delay_secs: config.retry_delay_secs,
            discovered: AtomicBool::new(false),
            registered: Mutex::new(HashMap::new()),
        }
    }

    /// Register the backend's standard curves and hand the resulting mappings
    /// back. Runs before stream registration: curve-carrying descriptors need
    /// the mapped sensor ids, and readings need the curve UUIDs.
    async fn sync_curves(&self, result: &mut SyncResult) {
        let curves = match self.backend.discover_standard_curves().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "Standard curve discovery failed");
                result.errors.push(format!("Curve discovery: {e}"));
                return;
            }
        };
        if curves.is_empty() {
            return;
        }
        let mappings = match self
            .api
            .register_standard_curves(self.backend.source_system(), &curves)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "Standard curve registration failed");
                result.errors.push(format!("Curve registration: {e}"));
                return;
            }
        };
        if let Err(e) = self.backend.apply_curve_mappings(&mappings).await {
            tracing::warn!(error = %e, "Applying curve mappings failed");
            result.errors.push(format!("Curve mappings: {e}"));
            return;
        }
        result
            .log
            .push(format!("Standard curves: {} registered", mappings.len()));
    }

    async fn discover(&self, result: &mut SyncResult, full: bool) {
        let descriptors = match self.backend.discover_streams().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "Stream discovery failed");
                result.errors.push(format!("Stream discovery: {e}"));
                return;
            }
        };

        let mut registered = 0usize;
        let mut unchanged = 0usize;
        for d in &descriptors {
            let req = RegisterStreamRequest {
                source_system: self.backend.source_system().to_string(),
                source_key: d.source_key.clone(),
                source_name: Some(d.source_name.clone()),
                source_path: Some(d.source_path.clone()),
                metadata: d.metadata.clone(),
                measurement_type: d.measurement_type.clone(),
                sensor_id: d.sensor_id,
                replicates: d.replicates.clone(),
            };
            // A descriptor identical to the last successfully registered one converges to the
            // same server state; re-sending it is pure write churn. A full sync re-asserts all.
            let req_hash = serde_json::to_vec(&req).ok().map(|b| fnv1a64(&b));
            if !full
                && let Some(h) = req_hash
                && self.registered.lock().is_ok_and(|m| m.get(&d.source_key) == Some(&h))
            {
                registered += 1;
                unchanged += 1;
                continue;
            }
            match self.api.register_stream(&req).await {
                Ok(stream) => {
                    registered += 1;
                    if let (Some(h), Ok(mut m)) = (req_hash, self.registered.lock()) {
                        m.insert(d.source_key.clone(), h);
                    }
                    if let Some(assignments) = &stream.replicates
                        && let Err(e) = self
                            .backend
                            .apply_replicate_assignments(&d.source_key, assignments)
                            .await
                    {
                        tracing::warn!(source_key = %d.source_key, error = %e, "Applying replicate assignments failed");
                        result
                            .errors
                            .push(format!("{}: replicate assignments", d.source_key));
                    }
                }
                Err(e) => {
                    tracing::warn!(source_key = %d.source_key, error = %e, "Stream registration failed");
                    result
                        .errors
                        .push(format!("{}: registration failed", d.source_key));
                }
            }
        }

        // Only latch when every descriptor registered; a partial pass (API
        // outage mid-registration) must be retried on the next cycle.
        if registered == descriptors.len() {
            self.discovered.store(true, Ordering::Relaxed);
        }
        result.log.push(format!(
            "Stream discovery: {} streams registered, {} unchanged",
            registered - unchanged,
            unchanged
        ));
    }

    /// Hand the backend the pinned replicate mapping each listed stream's
    /// metadata persists. The register response already delivered the fresh
    /// copy on cycles that ran discovery; this covers cycles that skipped it
    /// and the resync command, so index assignment never falls back to column
    /// position merely because discovery did not run in this process.
    async fn apply_persisted_assignments(&self, streams: &[DataStream]) {
        for s in streams {
            let Some(assignments) = ColumnAssignment::from_metadata(&s.metadata) else {
                continue;
            };
            if let Err(e) = self
                .backend
                .apply_replicate_assignments(&s.source_key, &assignments)
                .await
            {
                tracing::warn!(source_key = %s.source_key, error = %e, "Applying replicate assignments failed");
            }
        }
    }

    async fn sync_readings_once(&self, full: bool) -> Result<ReadingsOutcome, String> {
        let streams = self
            .api
            .list_streams(Some(self.backend.source_system()), Some(true))
            .await
            .map_err(|e| format!("list streams: {e}"))?;

        self.apply_persisted_assignments(&streams).await;

        // A reconciled backend re-reads its source's full content every cycle: the cursor is
        // what would truncate the payload below the stored maximum and turn a completeness
        // window into a mass withdrawal, so it is never applied there.
        let reconciled = self.backend.reconciled();
        let requests: Vec<StreamFetchRequest> = streams
            .iter()
            .map(|s| StreamFetchRequest {
                stream_id: s.id,
                source_key: s.source_key.clone(),
                since: if full || reconciled {
                    None
                } else {
                    s.last_data_time
                },
            })
            .collect();

        let mut fetched = self
            .backend
            .fetch_readings(&requests)
            .await
            .map_err(|e| format!("fetch readings: {e}"))?;

        // The digests of each stream's last cleanly-applied windowed pass, as the server
        // persisted them. Absent (older API, or no clean pass yet) means send in full.
        let server_digests: HashMap<Uuid, String> = streams
            .iter()
            .filter_map(|s| s.last_window_digest.clone().map(|d| (s.id, d)))
            .collect();

        let mut outcome = ReadingsOutcome {
            streams,
            readings_synced: 0,
            readings_skipped: 0,
            readings_held: 0,
            streams_with_data: 0,
            log: Vec::new(),
            errors: Vec::new(),
        };

        let skip = if full { &HashMap::new() } else { &server_digests };
        self.ingest_fetched(&mut fetched, false, skip, &mut outcome)
            .await;

        Ok(outcome)
    }

    /// `known_digests` holds each stream's last cleanly-applied window digest; a windowed
    /// payload whose content digest matches is skipped outright (the source is unchanged).
    /// Pass an empty map to disable skipping (full sync, resync).
    async fn ingest_fetched(
        &self,
        fetched: &mut [StreamReadings],
        overwrite: bool,
        known_digests: &HashMap<Uuid, String>,
        outcome: &mut ReadingsOutcome,
    ) {
        let mut skipped_unchanged = 0usize;
        for sr in fetched.iter_mut() {
            // An empty payload with no completeness claim is nothing to do. With a window it is
            // a claim the source holds nothing there, which the server must see: it either
            // no-ops (nothing stored) or refuses loudly (the store holds rows the claim says
            // do not exist), and silence would hide exactly that case.
            if sr.readings.is_empty() && sr.window.is_none() {
                continue;
            }
            // The handshake: identical content to the last cleanly-applied pass has nothing to
            // say. The server never persists a digest for a braked, held or rejected pass, so
            // those windows keep re-asserting until a person rules.
            let digest = window_digest(sr);
            if !overwrite
                && let Some(d) = &digest
                && known_digests.get(&sr.stream_id) == Some(d)
            {
                skipped_unchanged += 1;
                continue;
            }
            if let Some(w) = sr.window.as_mut() {
                w.content_digest = digest;
            }
            let opts = IngestOptions {
                overwrite,
                collection: sr.collection,
                audits: &sr.audits,
                window: sr.window.as_ref(),
            };
            let batch = self
                .api
                .ingest_readings_batched_with(sr.stream_id, &sr.readings, INGEST_BATCH_SIZE, opts)
                .await;
            outcome.readings_synced += batch.inserted;
            outcome.readings_skipped += batch.skipped;
            outcome.readings_held += batch.held;
            if batch.changed > 0 || batch.withdrawn > 0 {
                let line = format!(
                    "{}: converged on source ({} corrected, {} withdrawn)",
                    sr.source_key, batch.changed, batch.withdrawn
                );
                if outcome.log.len() < MAX_LOG_LINES {
                    outcome.log.push(line);
                } else {
                    tracing::info!(source_key = %sr.source_key, changed = batch.changed, withdrawn = batch.withdrawn, "Windowed convergence");
                }
            }
            if batch.failed_batches > 0 {
                outcome.errors.push(format!(
                    "{}: ingest failed, {} readings deferred to next cycle",
                    sr.source_key, batch.deferred
                ));
            }
            if batch.inserted > 0 {
                outcome.streams_with_data += 1;
            }
            if batch.inserted > 0 || batch.skipped > 0 || batch.held > 0 {
                let mut line = format!("{}: {} new readings", sr.source_key, batch.inserted);
                if batch.skipped > 0 {
                    line.push_str(&format!(
                        ", {} skipped ({})",
                        batch.skipped,
                        batch.skipped_reasons.join("; ")
                    ));
                }
                if batch.held > 0 {
                    line.push_str(&format!(
                        ", {} held pending audit acknowledgement",
                        batch.held
                    ));
                }
                if outcome.log.len() < MAX_LOG_LINES {
                    outcome.log.push(line);
                } else {
                    tracing::debug!(source_key = %sr.source_key, inserted = batch.inserted, skipped = batch.skipped, held = batch.held, "Stream ingest");
                }
            }
            // A wholly refused stream must surface as a partial cycle, which the runner derives
            // from a non-empty error list. Measured against what was sent: `inserted` counts rows
            // the upsert wrote, and is legitimately 0 whenever a backend re-sends a window
            // boundary it has already stored.
            if batch.skipped > 0 && batch.skipped as usize == sr.readings.len() {
                outcome.errors.push(format!(
                    "{}: all {} readings refused admission ({})",
                    sr.source_key,
                    batch.skipped,
                    batch.skipped_reasons.join("; ")
                ));
            }
            // Source-authored annotations after the readings they describe. A failure costs a
            // cycle, not the note: the source re-asserts its full set every pass and the
            // registration is idempotent per (source_system, source_key).
            if !sr.annotations.is_empty() {
                match self
                    .api
                    .register_annotations(self.backend.source_system(), &sr.annotations)
                    .await
                {
                    Ok(mappings) => {
                        let unpaired =
                            mappings.iter().filter(|m| m.status == "unpaired").count();
                        if unpaired > 0 && outcome.log.len() < MAX_LOG_LINES {
                            outcome.log.push(format!(
                                "{}: {} annotations deferred until the stream is paired",
                                sr.source_key, unpaired
                            ));
                        }
                    }
                    Err(e) => {
                        outcome.errors.push(format!(
                            "{}: annotation registration failed: {e}",
                            sr.source_key
                        ));
                    }
                }
            }
        }
        if skipped_unchanged > 0 {
            let line = format!("{skipped_unchanged} streams unchanged at source, not re-sent");
            if outcome.log.len() < MAX_LOG_LINES {
                outcome.log.push(line);
            } else {
                tracing::info!(skipped_unchanged, "Windowed streams unchanged at source");
            }
        }
    }

    /// RESYNC_STREAMS: fetch the named streams from the start of history and
    /// ingest with overwrite, leaving flags and sample links untouched.
    async fn resync_streams(
        &self,
        payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        #[derive(serde::Deserialize)]
        struct ResyncPayload {
            source_keys: Vec<String>,
            #[serde(default = "default_true")]
            overwrite: bool,
        }
        fn default_true() -> bool {
            true
        }

        let payload: ResyncPayload =
            serde_json::from_value(payload.ok_or("resync_streams requires a payload")?)
                .map_err(|e| format!("resync_streams payload: {e}"))?;
        if payload.source_keys.is_empty() {
            return Err("resync_streams: source_keys is empty".into());
        }

        let streams = self
            .api
            .list_streams(Some(self.backend.source_system()), None)
            .await
            .map_err(|e| format!("list streams: {e}"))?;

        self.apply_persisted_assignments(&streams).await;

        let wanted: std::collections::HashSet<&str> =
            payload.source_keys.iter().map(String::as_str).collect();
        let requests: Vec<StreamFetchRequest> = streams
            .iter()
            .filter(|s| wanted.contains(s.source_key.as_str()))
            .map(|s| StreamFetchRequest {
                stream_id: s.id,
                source_key: s.source_key.clone(),
                since: None,
            })
            .collect();
        let missing: Vec<&str> = payload
            .source_keys
            .iter()
            .map(String::as_str)
            .filter(|k| !requests.iter().any(|r| r.source_key == *k))
            .collect();
        if !missing.is_empty() {
            tracing::warn!(?missing, "resync_streams: source keys not registered");
        }

        let mut fetched = self
            .backend
            .fetch_readings(&requests)
            .await
            .map_err(|e| format!("fetch readings: {e}"))?;

        let mut outcome = ReadingsOutcome {
            streams: Vec::new(),
            readings_synced: 0,
            readings_skipped: 0,
            readings_held: 0,
            streams_with_data: 0,
            log: Vec::new(),
            errors: Vec::new(),
        };
        // A resync exists to repair server rows, so unchanged-content skipping is disabled.
        self.ingest_fetched(&mut fetched, payload.overwrite, &HashMap::new(), &mut outcome)
            .await;

        Ok(serde_json::json!({
            "streams_requested": payload.source_keys.len(),
            "streams_matched": requests.len(),
            "unmatched_source_keys": missing,
            "readings_synced": outcome.readings_synced,
            "readings_skipped": outcome.readings_skipped,
            "readings_held": outcome.readings_held,
            "errors": outcome.errors,
        }))
    }

    /// `retry_max` counts retries after the first attempt, matching the
    /// deployed RETRY_MAX semantics (RETRY_MAX=3 means up to 4 attempts).
    /// Returns the outcome plus the number of retries used.
    async fn sync_readings(&self, full: bool) -> Result<(ReadingsOutcome, u32), String> {
        let mut retries = 0u32;
        loop {
            match self.sync_readings_once(full).await {
                Ok(outcome) => return Ok((outcome, retries)),
                Err(e) if retries < self.retry_max => {
                    retries += 1;
                    tracing::warn!(retry = retries, max = self.retry_max, error = %e, "Readings sync failed, retrying");
                    tokio::time::sleep(Duration::from_secs(self.retry_delay_secs)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn sync_status_events(&self, streams: &[DataStream], result: &mut SyncResult) {
        let fetched = match self.backend.fetch_status_events(streams).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = %e, "Status event fetch failed");
                result.errors.push(format!("Status events: {e}"));
                return;
            }
        };

        for se in &fetched {
            if se.events.is_empty() {
                continue;
            }
            match self
                .api
                .ingest_status_events(se.stream_id, &se.events)
                .await
            {
                Ok(inserted) => result.status_events_synced += inserted,
                Err(e) => {
                    tracing::warn!(source_key = %se.source_key, error = %e, "Status event ingest failed");
                    result
                        .errors
                        .push(format!("{}: status event ingest failed", se.source_key));
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl SyncService for SyncDriver {
    async fn sync(
        &self,
        full: bool,
    ) -> Result<SyncResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut result = SyncResult::default();

        // Curves before registration and fetch: descriptors and readings both
        // depend on the mappings.
        self.sync_curves(&mut result).await;

        if full || !self.discovered.load(Ordering::Relaxed) || self.backend.rediscover_every_cycle()
        {
            self.discover(&mut result, full).await;
        }

        let readings = self.sync_readings(full).await;

        let outcome = match readings {
            Ok((outcome, retries)) => {
                result.readings_synced = outcome.readings_synced;
                result.readings_skipped = outcome.readings_skipped;
                result.readings_held = outcome.readings_held;
                result.log.push(format!(
                    "Readings sync: {} readings across {} streams, {} skipped, {} held ({} retries)",
                    outcome.readings_synced,
                    outcome.streams_with_data,
                    outcome.readings_skipped,
                    outcome.readings_held,
                    retries
                ));
                result.log.extend(outcome.log);
                result.errors.extend(outcome.errors);
                Some(outcome.streams)
            }
            Err(e) => {
                // Device health must keep flowing during a readings outage;
                // run the status phase before reporting the failure.
                let streams = self
                    .api
                    .list_streams(Some(self.backend.source_system()), Some(true))
                    .await
                    .unwrap_or_default();
                if !streams.is_empty() {
                    self.sync_status_events(&streams, &mut result).await;
                }
                return Err(e.into());
            }
        };

        if result.readings_synced > 0
            && let Err(e) = self.api.refresh_aggregates(full).await
        {
            tracing::warn!(error = %e, "Aggregate refresh failed");
            result.errors.push(format!("Aggregate refresh: {e}"));
        }

        if let Some(streams) = &outcome {
            self.sync_status_events(streams, &mut result).await;
        }

        cap_lines(&mut result.errors);
        Ok(result)
    }

    async fn handle_command(
        &self,
        command: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        if command == commands::RESYNC_STREAMS {
            return self.resync_streams(payload).await;
        }
        self.backend.handle_command(command, payload).await
    }

    fn river_data_client(&self) -> Option<&RiverDataClient> {
        Some(&self.api)
    }
}

#[cfg(test)]
mod tests {
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
        });
        assert_ne!(window_digest(&a), window_digest(&b));
    }
}
