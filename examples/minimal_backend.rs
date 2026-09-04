//! A sync service that pushes a synthetic temperature signal.
//!
//! Run with: cargo run --example minimal_backend --features client

use river_data_core::chrono::{Duration, Utc};
use river_data_core::client::{BackendError, SourceBackend, run_sync_service};
use river_data_core::models::{
    IngestReading, StreamDescriptor, StreamFetchRequest, StreamReadings,
};
use river_data_core::serde_json::json;

struct DemoBackend;

#[river_data_core::async_trait]
impl SourceBackend for DemoBackend {
    fn source_system(&self) -> &str {
        "demo"
    }

    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
        Ok(vec![StreamDescriptor {
            source_key: "demo-temperature".to_string(),
            source_name: "Demo Temperature".to_string(),
            source_path: "demo/lab/temperature".to_string(),
            metadata: json!({ "units": "degC" }),
            measurement_type: Some("continuous".to_string()),
            sensor_id: None,
            replicates: None,
            decimal_places: None,
        }])
    }

    async fn fetch_readings(
        &self,
        requests: &[StreamFetchRequest],
    ) -> Result<Vec<StreamReadings>, BackendError> {
        let now = Utc::now();
        Ok(requests
            .iter()
            .map(|req| {
                // Resume from the stream's cursor, or one hour back on the first sync
                let start = req.since.unwrap_or(now - Duration::hours(1));
                let mut readings = Vec::new();
                let mut t = start + Duration::minutes(10);
                while t <= now {
                    let hours = t.timestamp() as f64 / 3600.0;
                    readings.push(IngestReading::new(t, 15.0 + 5.0 * hours.sin()));
                    t += Duration::minutes(10);
                }
                StreamReadings::new(req.stream_id, req.source_key.clone(), readings)
            })
            .collect())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_sync_service(|_config| async { Ok(Box::new(DemoBackend) as Box<dyn SourceBackend>) })
}
