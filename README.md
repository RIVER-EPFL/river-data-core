# river-data-core

**Feed your instrument or spreadsheet data into river-data by implementing two functions.**

river-data-core is the shared library for the [river-data](https://github.com/RIVER-EPFL)
platform: the sync client and the wire protocol both ends share.
A sync service built on it registers its data streams once, then pushes new readings on a
schedule. Enrollment, heartbeats, retries, token rotation, batching and remote commands
(pause, resume, trigger a full sync from the dashboard) are all handled for you.

## Install

```toml
[dependencies]
river-data-core = { version = "0.9", features = ["client"] }
```

Common companion crates (`chrono`, `uuid`, `serde_json`, `tracing`, `async_trait`) are
re-exported at the crate root, so one dependency is enough to start.

## Quick start

A sync service answers two questions about your data source: what streams exist, and what
new readings arrived since the last sync. You answer them by implementing the
`SourceBackend` trait. The full program below pushes a synthetic temperature signal; it is
[examples/minimal_backend.rs](examples/minimal_backend.rs) in this repository.

First, describe your streams. A stream is one series of readings, ie. one column of a
spreadsheet or one sensor channel:

```rust
async fn discover_streams(&self) -> Result<Vec<StreamDescriptor>, BackendError> {
    Ok(vec![StreamDescriptor {
        source_key: "demo-temperature".to_string(),
        source_name: "Demo Temperature".to_string(),
        source_path: "demo/lab/temperature".to_string(),
        metadata: json!({ "units": "degC" }),
        measurement_type: Some("continuous".to_string()),
        sensor_id: None,
        replicates: None,
    }])
}
```

`source_key` is the stable identifier on your side (a column name, a location id).
Keep it stable across restarts: a changed key registers as a new, empty stream.
`measurement_type` is `"continuous"` for logger data or `"spot"` for grab samples.
`sensor_id` names the instrument behind the stream when your backend already knows it;
leave it `None` and the server resolves the instrument from the metadata serial when the
stream is imported or paired. `replicates` declares a replicate family (grab samples taken in
several vials at one instant); `None` for a plain series.

Second, fetch new readings. Each request carries `since`, the time of the newest reading
river-data already has for that stream, so you only return what is new. This is the
[examples/csv_folder.rs](examples/csv_folder.rs) version, one stream per `time,value` file:

```rust
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
```

Third, run it:

```rust
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_sync_service(|_config| async { Ok(Box::new(DemoBackend) as Box<dyn SourceBackend>) })
}
```

Ask a river-data administrator to create sync credentials for you (Sync Services page in
the dashboard), then set three environment variables and start the service:

```bash
export API_BASE_URL=https://river-data.example.org
export SERVICE_CLIENT_ID=svc_demo
export SERVICE_CLIENT_SECRET=your-secret
cargo run --example minimal_backend --features client
```

The service enrolls itself on startup, syncs every five minutes, and shows up on the
dashboard with its status and per-cycle event log.

One step remains before the data appears anywhere: an administrator pairs each new
stream to a site parameter (Streams page in the dashboard). Until then readings are
stored but belong to no site, so charts and exports show nothing. Ask for pairing once
your streams are registered; syncing continues normally either way and the paired
history is backfilled.

## Usage

**Cursors and full syncs.** On the first sync `since` is `None`: return your full history
(or a sensible window). After that, `since` is the newest ingested reading per stream.
A full sync (triggered from the dashboard) passes `None` again; re-sent readings are
deduplicated server-side, so returning overlap is safe.

**Reading files.** [examples/csv_folder.rs](examples/csv_folder.rs) syncs a folder of
`time,value` CSV files, one stream per file (useful as a template for lab exports).
It reads the folder named by `DATA_DIR` (default `./data`).

**Mutable sources.** A source that is edited in place (a portal database) cannot be synced
by cursor. Declare `reconciled()` and attach a completeness window to each fetch; the server
diffs stored content against the payload, corrects changed values and withdraws absent rows.
[examples/reconciled_backend.rs](examples/reconciled_backend.rs) is the template.

**Status events.** Override `fetch_status_events` to report device telemetry (battery,
signal, reachability) alongside readings. The default reports nothing.

**Custom commands.** Override `handle_command` to react to commands sent from the
dashboard beyond the built-in sync/pause/resume set.

**Real services.** [river-data-sync-vaisala](https://github.com/RIVER-EPFL/river-data-sync-vaisala)
is an HTTP source with status events;
[river-data-rshiny](https://github.com/RIVER-EPFL/river-data-rshiny) reads three MySQL portal
schemas behind one binary.

**Onboarding a new source.** [docs/sync-service-onboarding.md](docs/sync-service-onboarding.md)
covers credentials, the control plane, stream identity, replicate families, standard curves,
reconciled sources, pairing and migration, with the two real services as worked examples.

**Full control.** If the driver's cycle does not fit your source, implement the
`SyncService` trait instead and drive `SyncServiceRunner` from your own `main`.

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `API_BASE_URL` | river-data API URL | required |
| `SERVICE_CLIENT_ID` | Enrollment client id | required |
| `SERVICE_CLIENT_SECRET` | Enrollment client secret | required |
| `INSTANCE_ID` | Distinguishes multiple instances of one service | `default` |
| `SYNC_INTERVAL_SECONDS` | Time between sync cycles | `300` |
| `HEARTBEAT_INTERVAL_SECONDS` | Time between heartbeats | `30` |
| `ENROLLMENT_RETRY_SECONDS` | Wait between enrollment attempts | `10` |
| `RETRY_MAX` | Readings sync retries after the first attempt | `3` |
| `RETRY_DELAY_SECONDS` | Wait between attempts | `60` |
| `RUST_LOG` | Log filter | `info` |

Variables are also read from a `.env` file in the working directory.

## Features

- `client`: sync service runner, driver and HTTP client (reqwest)
- `openapi`: `ToSchema` derives on the protocol types, for a host API that publishes a spec

The control plane's server side lives in river-data-api, which owns the database schema those
handlers query. This crate defines the protocol they speak.

## License

MIT
