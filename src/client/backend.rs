use crate::models::{DataStream, StreamDescriptor, StreamFetchRequest, StreamReadings, StreamStatusEvents};

pub type BackendError = Box<dyn std::error::Error + Send + Sync>;

/// Source-specific extraction logic. Implement this to build a sync service;
/// registration, cursors, batching, retries and reporting are the driver's job.
#[async_trait::async_trait]
pub trait SourceBackend: Send + Sync + 'static {
    /// The source_system string streams register under (ie. "vaisala", "cnet").
    fn source_system(&self) -> &str;

    /// Run discovery on every cycle instead of once at startup plus full syncs.
    /// Return true when discovery is cheap and new streams appear without
    /// operator action (ie. rows added to a portal database).
    fn rediscover_every_cycle(&self) -> bool {
        false
    }

    /// Enumerate the streams this source provides.
    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError>;

    /// Fetch new readings. Receives all requests in one call so the backend
    /// can batch upstream queries; each stream carries its own cursor.
    async fn fetch_readings(
        &self,
        requests: &[StreamFetchRequest],
    ) -> Result<Vec<StreamReadings>, BackendError>;

    /// Device or status telemetry. Default: none.
    async fn fetch_status_events(
        &self,
        _streams: &[DataStream],
    ) -> Result<Vec<StreamStatusEvents>, BackendError> {
        Ok(Vec::new())
    }

    /// Custom command handling, forwarded from the runner.
    async fn handle_command(
        &self,
        command: &str,
        _payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, BackendError> {
        Err(format!("Unknown command: {command}").into())
    }
}
