//! A sync service that replicates a mutable source: a small table of spot measurements that is
//! re-read in full every cycle and sent with a completeness window, so a correction or a
//! deletion at the source converges in river-data. Also reports a status line per stream and
//! accepts a custom command from the dashboard.
//!
//! Run with: cargo run --example reconciled_backend --features client

use std::sync::Mutex;

use river_data_core::chrono::{DateTime, TimeZone, Utc};
use river_data_core::client::{BackendError, SourceBackend, run_sync_service};
use river_data_core::models::{
    DataStream, IngestReading, IngestStatusEvent, SourceWindow, StreamDescriptor,
    StreamFetchRequest, StreamReadings, StreamStatusEvents,
};
use river_data_core::serde_json::{Value, json};

/// A source cell as read: a value, an empty cell, or one the backend could not decode.
#[derive(Clone, Copy)]
enum Cell {
    Value(f64),
    Empty,
    Undecodable,
}

struct TableBackend {
    rows: Mutex<Vec<(DateTime<Utc>, Cell)>>,
}

impl TableBackend {
    fn seeded() -> Self {
        let at = |day: u32| Utc.with_ymd_and_hms(2026, 6, day, 9, 0, 0).unwrap();
        Self {
            rows: Mutex::new(vec![
                (at(1), Cell::Value(4.2)),
                (at(2), Cell::Empty),
                (at(3), Cell::Value(4.9)),
                (at(4), Cell::Undecodable),
            ]),
        }
    }
}

#[river_data_core::async_trait]
impl SourceBackend for TableBackend {
    fn source_system(&self) -> &str {
        "table"
    }

    /// The source is edited in place, so every cycle re-reads it whole and the driver asks
    /// for it without a cursor.
    fn reconciled(&self) -> bool {
        true
    }

    async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
        Ok(vec![StreamDescriptor {
            source_key: "doc".to_string(),
            source_name: "DOC".to_string(),
            source_path: "table/lab/doc".to_string(),
            metadata: json!({ "units": "mg/L" }),
            // A completeness window is only accepted on a spot stream.
            measurement_type: Some("spot".to_string()),
            sensor_id: None,
            replicates: None,
            decimal_places: None,
        }])
    }

    async fn fetch_readings(
        &self,
        requests: &[StreamFetchRequest],
    ) -> Result<Vec<StreamReadings>, BackendError> {
        let rows = self.rows.lock().map_err(|_| "rows lock poisoned")?;
        let mut out = Vec::new();
        for req in requests {
            let mut readings = Vec::new();
            let mut dropped_times = Vec::new();
            for (time, cell) in rows.iter() {
                match cell {
                    Cell::Value(v) => readings.push(IngestReading::new(*time, *v)),
                    // An empty cell is an absent measurement: a stored row at this time
                    // is withdrawn.
                    Cell::Empty => {}
                    // A cell the backend saw but could not carry: the server retains
                    // whatever it holds at this time rather than withdrawing it.
                    Cell::Undecodable => dropped_times.push(*time),
                }
            }
            let mut sr = StreamReadings::new(req.stream_id, req.source_key.clone(), readings);
            sr.window = Some(SourceWindow {
                from: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                to: Utc::now(),
                source_rows_read: rows.len() as u64,
                dropped_times,
                // Stamped by the driver.
                content_digest: None,
            });
            out.push(sr);
        }
        Ok(out)
    }

    async fn fetch_status_events(
        &self,
        streams: &[DataStream],
    ) -> Result<Vec<StreamStatusEvents>, BackendError> {
        let rows = self.rows.lock().map_err(|_| "rows lock poisoned")?;
        let undecodable = rows
            .iter()
            .filter(|(_, c)| matches!(c, Cell::Undecodable))
            .count();
        Ok(streams
            .iter()
            .map(|s| StreamStatusEvents {
                stream_id: s.id,
                source_key: s.source_key.clone(),
                events: vec![IngestStatusEvent {
                    time: Utc::now(),
                    value: format!("rows={} undecodable={undecodable}", rows.len()),
                }],
            })
            .collect())
    }

    /// `correct_value {"time": "2026-06-03T09:00:00Z", "value": 5.1}` edits one source cell;
    /// the next cycle's window carries the correction.
    async fn handle_command(
        &self,
        command: &str,
        payload: Option<Value>,
    ) -> Result<Value, BackendError> {
        if command != "correct_value" {
            return Err(format!("Unknown command: {command}").into());
        }
        let payload = payload.ok_or("correct_value requires a payload")?;
        let time = payload["time"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .ok_or("correct_value: time must be RFC 3339")?
            .to_utc();
        let value = payload["value"]
            .as_f64()
            .ok_or("correct_value: value must be a number")?;
        let mut rows = self.rows.lock().map_err(|_| "rows lock poisoned")?;
        match rows.iter_mut().find(|(t, _)| *t == time) {
            Some(row) => row.1 = Cell::Value(value),
            None => rows.push((time, Cell::Value(value))),
        }
        Ok(json!({ "time": time, "value": value }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_sync_service(|_config| async {
        Ok(Box::new(TableBackend::seeded()) as Box<dyn SourceBackend>)
    })
}
