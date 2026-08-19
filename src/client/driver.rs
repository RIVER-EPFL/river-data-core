use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::client::backend::SourceBackend;
use crate::client::river_data_client::RiverDataClient;
use crate::client::service::SyncService;
use crate::models::{
    DataStream, RegisterStreamRequest, RunnerConfig, StreamFetchRequest, SyncResult,
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
}

struct ReadingsOutcome {
    streams: Vec<DataStream>,
    readings_synced: u64,
    readings_skipped: u64,
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
        }
    }

    async fn discover(&self, result: &mut SyncResult) {
        let descriptors = match self.backend.discover_streams().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "Stream discovery failed");
                result.errors.push(format!("Stream discovery: {e}"));
                return;
            }
        };

        let mut registered = 0usize;
        for d in &descriptors {
            let req = RegisterStreamRequest {
                source_system: self.backend.source_system().to_string(),
                source_key: d.source_key.clone(),
                source_name: Some(d.source_name.clone()),
                source_path: Some(d.source_path.clone()),
                metadata: d.metadata.clone(),
                measurement_type: d.measurement_type.clone(),
                sensor_id: d.sensor_id,
            };
            match self.api.register_stream(&req).await {
                Ok(_) => registered += 1,
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
        result
            .log
            .push(format!("Stream discovery: {registered} streams registered"));
    }

    async fn sync_readings_once(&self, full: bool) -> Result<ReadingsOutcome, String> {
        let streams = self
            .api
            .list_streams(Some(self.backend.source_system()), Some(true))
            .await
            .map_err(|e| format!("list streams: {e}"))?;

        let requests: Vec<StreamFetchRequest> = streams
            .iter()
            .map(|s| StreamFetchRequest {
                stream_id: s.id,
                source_key: s.source_key.clone(),
                since: if full { None } else { s.last_data_time },
            })
            .collect();

        let fetched = self
            .backend
            .fetch_readings(&requests)
            .await
            .map_err(|e| format!("fetch readings: {e}"))?;

        let mut outcome = ReadingsOutcome {
            streams,
            readings_synced: 0,
            readings_skipped: 0,
            streams_with_data: 0,
            log: Vec::new(),
            errors: Vec::new(),
        };

        for sr in &fetched {
            if sr.readings.is_empty() {
                continue;
            }
            let batch = self
                .api
                .ingest_readings_batched(sr.stream_id, &sr.readings, INGEST_BATCH_SIZE)
                .await;
            outcome.readings_synced += batch.inserted;
            outcome.readings_skipped += batch.skipped;
            if batch.failed_batches > 0 {
                outcome.errors.push(format!(
                    "{}: ingest failed, {} readings deferred to next cycle",
                    sr.source_key, batch.deferred
                ));
            }
            if batch.inserted > 0 {
                outcome.streams_with_data += 1;
            }
            if batch.inserted > 0 || batch.skipped > 0 {
                let mut line = format!("{}: {} new readings", sr.source_key, batch.inserted);
                if batch.skipped > 0 {
                    line.push_str(&format!(
                        ", {} skipped ({})",
                        batch.skipped,
                        batch.skipped_reasons.join("; ")
                    ));
                }
                if outcome.log.len() < MAX_LOG_LINES {
                    outcome.log.push(line);
                } else {
                    tracing::debug!(source_key = %sr.source_key, inserted = batch.inserted, skipped = batch.skipped, "Stream ingest");
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
        }

        Ok(outcome)
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

        if full || !self.discovered.load(Ordering::Relaxed) || self.backend.rediscover_every_cycle()
        {
            self.discover(&mut result).await;
        }

        let readings = self.sync_readings(full).await;

        let outcome = match readings {
            Ok((outcome, retries)) => {
                result.readings_synced = outcome.readings_synced;
                result.readings_skipped = outcome.readings_skipped;
                result.log.push(format!(
                    "Readings sync: {} readings across {} streams, {} skipped ({} retries)",
                    outcome.readings_synced,
                    outcome.streams_with_data,
                    outcome.readings_skipped,
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
        self.backend.handle_command(command, payload).await
    }

    fn river_data_client(&self) -> Option<&RiverDataClient> {
        Some(&self.api)
    }
}
