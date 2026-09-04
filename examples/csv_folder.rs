//! Sync readings from a folder of CSV files.
//!
//! Each `.csv` file in DATA_DIR becomes one stream, named after the file.
//! Rows are `time,value` with RFC 3339 timestamps:
//!
//!   2026-08-11T10:00:00Z,12.5
//!
//! Run with: DATA_DIR=./data cargo run --example csv_folder --features client

use std::path::{Path, PathBuf};

use river_data_core::chrono::{DateTime, Utc};
use river_data_core::client::{BackendError, SourceBackend, run_sync_service};
use river_data_core::models::{
    IngestReading, StreamDescriptor, StreamFetchRequest, StreamReadings,
};
use river_data_core::serde_json::json;

struct CsvFolderBackend {
    dir: PathBuf,
}

impl CsvFolderBackend {
    fn csv_files(&self) -> Result<Vec<PathBuf>, BackendError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "csv") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
}

fn stream_key(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn parse_row(line: &str) -> Option<(DateTime<Utc>, f64)> {
    let (time_s, value_s) = line.split_once(',')?;
    let time = DateTime::parse_from_rfc3339(time_s.trim()).ok()?.to_utc();
    let value = value_s.trim().parse().ok()?;
    Some((time, value))
}

#[river_data_core::async_trait]
impl SourceBackend for CsvFolderBackend {
    fn source_system(&self) -> &str {
        "csv-folder"
    }

    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
        Ok(self
            .csv_files()?
            .iter()
            .map(|path| {
                let key = stream_key(path);
                StreamDescriptor {
                    source_name: key.clone(),
                    source_path: format!("csv-folder/{key}"),
                    source_key: key,
                    metadata: json!({ "file": path.to_string_lossy() }),
                    measurement_type: None,
                    sensor_id: None,
                    replicates: None,
                    decimal_places: None,
                }
            })
            .collect())
    }

    async fn fetch_readings(
        &self,
        requests: &[StreamFetchRequest],
    ) -> Result<Vec<StreamReadings>, BackendError> {
        let mut out = Vec::new();
        for req in requests {
            let path = self.dir.join(format!("{}.csv", req.source_key));
            let content = std::fs::read_to_string(&path)?;

            // Keep only rows newer than the cursor so re-syncs don't re-send
            let readings: Vec<IngestReading> = content
                .lines()
                .filter_map(parse_row)
                .filter(|(time, _)| req.since.is_none_or(|s| *time > s))
                .map(|(time, value)| IngestReading::new(time, value))
                .collect();

            out.push(StreamReadings::new(
                req.stream_id,
                req.source_key.clone(),
                readings,
            ));
        }
        Ok(out)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_sync_service(|_config| async {
        let dir = PathBuf::from(river_data_core::env::string_or("DATA_DIR", "./data"));
        Ok(Box::new(CsvFolderBackend { dir }) as Box<dyn SourceBackend>)
    })
}
