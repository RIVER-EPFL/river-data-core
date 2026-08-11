use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::client::backend::SourceBackend;
use crate::client::river_data_client::RiverDataClient;
use crate::client::service::SyncService;
use crate::models::{DataStream, RegisterStreamRequest, RunnerConfig, StreamFetchRequest, SyncResult};

pub const INGEST_BATCH_SIZE: usize = 1000;

/// Sync-event log lines per cycle; further per-stream detail goes to debug logging.
const MAX_LOG_LINES: usize = 50;

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
    log: Vec<String>,
    errors: Vec<String>,
}

impl SyncDriver {
    pub fn new(backend: Box<dyn SourceBackend>, api: RiverDataClient, config: &RunnerConfig) -> Self {
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
            };
            match self.api.register_stream(&req).await {
                Ok(_) => registered += 1,
                Err(e) => {
                    tracing::warn!(source_key = %d.source_key, error = %e, "Stream registration failed");
                    result.errors.push(format!("{}: registration failed", d.source_key));
                }
            }
        }

        self.discovered.store(true, Ordering::Relaxed);
        result.log.push(format!("Stream discovery: {registered} streams registered"));
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
            if batch.failed_batches > 0 {
                outcome
                    .errors
                    .push(format!("{}: {} ingest batches failed", sr.source_key, batch.failed_batches));
            }
            if batch.inserted > 0 {
                if outcome.log.len() < MAX_LOG_LINES {
                    outcome
                        .log
                        .push(format!("{}: {} new readings", sr.source_key, batch.inserted));
                } else {
                    tracing::debug!(source_key = %sr.source_key, inserted = batch.inserted, "New readings");
                }
            }
        }

        Ok(outcome)
    }

    /// `retry_max` counts total attempts, matching the RETRY_MAX env semantics.
    async fn sync_readings(&self, full: bool) -> Result<ReadingsOutcome, String> {
        let mut attempt = 1u32;
        loop {
            match self.sync_readings_once(full).await {
                Ok(outcome) => return Ok(outcome),
                Err(e) if attempt < self.retry_max => {
                    tracing::warn!(attempt, max = self.retry_max, error = %e, "Readings sync failed, retrying");
                    attempt += 1;
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
            match self.api.ingest_status_events(se.stream_id, &se.events).await {
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
    async fn sync(&self, full: bool) -> Result<SyncResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut result = SyncResult::default();

        if full || !self.discovered.load(Ordering::Relaxed) {
            self.discover(&mut result).await;
        }

        let outcome = self.sync_readings(full).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
        result.readings_synced = outcome.readings_synced;
        result.log.extend(outcome.log);
        result.errors.extend(outcome.errors);

        if result.readings_synced > 0
            && let Err(e) = self.api.refresh_aggregates(full).await
        {
            tracing::warn!(error = %e, "Aggregate refresh failed");
            result.errors.push(format!("Aggregate refresh: {e}"));
        }

        self.sync_status_events(&outcome.streams, &mut result).await;

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
