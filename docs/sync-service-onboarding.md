# Sync service onboarding

How to connect a new data source to river-data by writing a sync service on `river-data-core`,
and how to run it once it exists. The two services in production, river-data-vaisala (an HTTP
logger API) and river-data-rshiny (three MySQL portal schemas), are the worked examples at the
end.

Every Rust sample on this page is a verbatim excerpt of a file under [`examples/`](../examples)
in this repository, and `tests/docs_test.rs` fails when one drifts. The examples build under
`cargo build --examples --features client`, so a sample that stops compiling fails CI rather
than misleading the next author.

## What a sync service is

A sync service is one process that answers two questions about a source, on a schedule: which
streams exist, and what the source holds for each of them. Everything else is the library's:

- `SourceBackend` (`src/client/backend.rs`) is the trait you implement. Two methods are
  required, `source_system` and `discover_streams` plus `fetch_readings`; the rest have
  defaults.
- `SyncDriver` (`src/client/driver.rs`) runs the cycle: curve registration, stream
  registration, cursor handling, batching, the digest handshake, annotation registration,
  aggregate refresh, status events and the per-cycle report.
- `SyncServiceRunner` (`src/client/runner.rs`) owns the control plane: enrollment, heartbeats,
  session tokens, pause state, cadence, and command dispatch.
- `run_sync_service` (`src/client/bootstrap.rs`) wires the three together from environment
  variables and blocks until SIGINT or SIGTERM. A service's `main` is one call to it.

`SyncService` (`src/client/service.rs`) is the escape hatch below `SourceBackend` for a source
whose cycle does not fit the driver. Neither shipped service needs it.

## Stream identity

A stream is one series of readings and is identified by `(source_system, source_key)`, unique
in `data_streams`. `source_system` is what `SourceBackend::source_system` returns
(`vaisala`, `cnet`, `metalp`, `nomis`); `source_key` is the stable identifier on the source's
side: a viewLinc location id (`1270`), a portal `station:column` pair (`VAD:WTW_DO_mgL_1`), or
a replicate family (`VAD:DOC_avg_ppb:reps`). Two systems using the same external id never
collide, and a changed key registers a new, empty stream, so keep keys stable across restarts.

Registration is `POST /api/streams/register`, an upsert on that pair. The driver registers
every descriptor on the first cycle and on a full sync; between those it re-registers only a
descriptor whose content changed, and the server writes only fields that differ. A descriptor
carries:

- `source_name` and `source_path`: the label and the hierarchy path (`viewLinc/BREATHE/Martigny/Depth`,
  `cnet/VAD/WTW_DO_mgL_1`). The path is parsed server-side by the pairing wizard for site
  discovery.
- `metadata`: any JSON. Vaisala puts the device serials and units here; the portals put
  station, parameter and coordinates. The pairing wizard reads `hierarchy` and `coordinates`
  when they are present.
- `measurement_type`: `"continuous"` for logger data, `"spot"` for grab samples. `None` never
  clears a value an operator set on the stream.
- `sensor_id`: the instrument behind the stream when the backend already knows it; required
  when readings will carry standard curve claims (the API admits a claim only when the reading's
  sensor is the curve's sensor).
- `replicates`: a `ReplicateSpec` for a stream whose readings are replicate groups (below).

Readings are keyed `(stream_id, time, replicate_index)`. Attribution to a site and a parameter
comes from the stream's pairing (`data_streams.site_parameter_id`) and never from the request:
until an administrator pairs the stream, its readings are stored but belong to no site, and
charts, exports and the public API show nothing. Pairing backfills the stored history.

## Credentials and the control plane

An administrator mints credentials for the service type once:

```
POST /api/sync/credentials      {"service_type": "campbell"}
                             -> {"client_id": "...", "client_secret": "..."}
```

The secret is returned once and stored hashed. `service_type` is fixed by the credential, not by
the service, and a running instance is the row `(service_type, instance_id)` in
`sync_services`; two pods of one service differ by `INSTANCE_ID`.

The runner then does the following without any code on your side:

1. **Enroll**: `POST /api/sync/enroll {client_id, client_secret, instance_id}`, retried every
   `ENROLLMENT_RETRY_SECONDS` until it succeeds. The response carries `service_id`, a session
   token, the persisted `paused` flag and the operator's `sync_interval_secs`.
2. **Heartbeat** every `HEARTBEAT_INTERVAL_SECONDS`: `POST /api/sync/heartbeat` with the session
   token, reporting `idle`, `syncing` or `paused` and the current operation. The reply rotates
   the token (default lifetime 900 s, `SYNC_SESSION_TOKEN_TTL_SECS` on the API), reconciles pause
   and cadence, and delivers pending commands. A 401 on the heartbeat re-enrolls.
3. **Sync** every `SYNC_INTERVAL_SECONDS`, or at the cadence the operator set through
   `PATCH /api/sync/services/{id} {"sync_interval_secs": 3600}` (floored at 30 s, `null` returns
   the service to its own setting). Each cycle is one `sync_events` row: `running`, then
   `completed`, `partial` (some errors) or `failed`, with counts, log lines and errors. That row
   is the operator's view of the cycle, so the driver writes the server's refusal text into it
   rather than only the process log.

Health on the System page is heartbeat age against the API's thresholds: `healthy` under
`SYNC_HEALTH_HEALTHY_SECS` (90), `warning` under `SYNC_HEALTH_WARNING_SECS` (300), `stale`
beyond; the `sync_stale` notification trigger reads the same age. A dead service writes no
events, so this is the only signal that catches it.

### Configuration

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

Variables are read from the environment and from a `.env` file in the working directory.
Source-specific settings are yours to read; `river_data_core::env` has `require`, `string_or`,
`parse_or` and `bool_or` for that, and the backend-building closure receives the parsed
`RunnerConfig` if it needs any of the above.

### Commands

Issued from the System page or `POST /api/sync/services/{id}/commands {"command": ..., "payload": ...}`
and picked up on the next heartbeat:

| Command | Effect |
|---------|--------|
| `trigger_sync` | One incremental cycle now |
| `trigger_full_sync` | Rediscover every stream and fetch with no cursor |
| `pause` / `resume` | Stop or restart scheduled cycles; persisted, so a restart cannot undo a pause. Triggered syncs still run |
| `resync_streams` | `{"source_keys": [...], "overwrite": true}`: re-fetch the named streams from the start of history and ingest with overwrite, leaving flags and sample links untouched |
| anything else | Forwarded to `SourceBackend::handle_command`; its `Ok` value or error becomes the command's result |

## The minimal backend: an append-only source

An append-only source (a logger, a folder of exports) is asked for what is newer than the
server's cursor. [`examples/minimal_backend.rs`](../examples/minimal_backend.rs) is the whole
program; the three parts that matter are below.

Describe the streams:

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
        decimal_places: None,
    }])
}
```

Return the readings newer than each stream's cursor. Every request carries `since`, the newest
instant river-data holds for that stream, `None` on a new stream or a full sync. This is
[`examples/csv_folder.rs`](../examples/csv_folder.rs), one stream per `time,value` file:

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

Run it:

```rust
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_sync_service(|_config| async { Ok(Box::new(DemoBackend) as Box<dyn SourceBackend>) })
}
```

What the driver does with the result:

- Readings are sorted by `(time, replicate_index)` and sent to `POST /api/ingest` in chunks of
  1000, never splitting a run of identical timestamps across chunks. A failed chunk stops that
  stream for the cycle (the rest is deferred to the next one), because a later chunk landing
  would move the cursor past the gap.
- The server's cursor (`data_streams.last_data_time`) advances to the newest instant in the
  batch. Returning overlap is safe: an existing key is left alone unless the request says
  `overwrite`.
- A source failure in `fetch_readings` is retried `RETRY_MAX` times, `RETRY_DELAY_SECONDS`
  apart; exhausting them fails the cycle, but status events still run first so device health
  keeps flowing through a readings outage.
- Aggregates are refreshed once per cycle when anything was inserted.

### Reading the ingest response

`IngestOutcome` and the cycle's `SyncResult` carry what the API reported:

| Count | Meaning |
|-------|---------|
| `inserted` | Rows written |
| `skipped`, `skipped_reasons` | Rows refused admission (timestamp out of window, non-finite value, unknown `measurement_type` or `calibration_id`). Dropped, not deferred: the cursor advances past them, and the count goes on the cycle's event so the loss is queryable. A stream whose every reading is refused makes the cycle `partial` |
| `held` | Always 0. The replicate audit admits every group and records a disagreement as a review hold (ADR 0002); nothing is withheld and nothing is re-sent. The field stays on the wire for older images |
| `changed`, `withdrawn`, `unchanged` | Windowed diff counts (below) |

## Declaring what the readings are

`IngestReading::new(time, value)` is a reading at replicate index 0 with nothing else claimed;
the server resolves sensor, calibration and deployment from the stream's pairing. The other
fields are for a backend that knows more:

- `measurement_type` per reading overrides the stream's declaration for that row.
- `sensor_id`, `calibration_id`, `deployment_id`: explicit attribution, for a source that
  carries it.
- `standard_curve_id`: the lab curve the source applied to this reading, as registered below.

### Replicate families

A stream whose readings are replicate groups (three DOC vials at one instant) declares a
`ReplicateSpec` on its descriptor and sets `measurement_type: "spot"`:

- `source_columns` are the source's member columns. The server pins each column to a
  `replicate_index` (append-only across re-registrations; a column that disappears keeps its
  index reserved, marked `retired`) and returns the mapping on the register response, which the
  driver hands to `apply_replicate_assignments` before the first fetch, along with the copy the
  stream's metadata persists on cycles that skipped discovery. Assign each value's
  `replicate_index` from that mapping, never from column position: a portal that drops a middle
  column must not renumber the survivors.
- `portal_mean_column`, `portal_sd_column`: the source's own statistics, sent per instant as a
  `GroupAudit` on `StreamReadings.audits` (`expected_mean`, `expected_sd`, `expected_n`). The
  server recomputes from the members and records a disagreement as a review hold; it never
  refuses the group.
- `sd_estimator`: `"sample"` or `"population"` when the source declares which divisor its sd
  column uses. Leave it `None` otherwise; the server never infers one.
- `curve_ref_column`, `calc`: provenance of the source's calculation, stored on the stream.

Set `StreamReadings.collection = true` so the server groups the payload per instant and
materialises `samples` rows.

### Standard curves and annotations

A source with lab calibration curves registers them before its streams: return them from
`discover_standard_curves` (`StandardCurveUpsert`: `source_key`, `instrument_label`, `slope`,
`intercept`, `r_squared`, `name`), idempotent per `(source_system, source_key)`. The driver
posts each to `POST /api/standard_curves/register` and hands the resulting `CurveMapping`s
(curve id, the lab instrument the API found or created for the label, whether the curve was
superseded) to `apply_curve_mappings`. Readings then name the curve by `standard_curve_id`, and
the descriptor of any stream carrying such claims names the curve's `sensor_id`.

`StreamReadings.annotations` carries source-authored notes on instants (`AnnotationUpsert`),
registered after the readings through `POST /api/annotations/register`, idempotent per
`(source_system, source_key)`. An annotation on an unpaired stream is reported `unpaired` and
re-asserted on a later cycle.

## A reconciled backend: a mutable source

A source that is edited in place (a portal database where a row is corrected months after the
visit) cannot be synced by cursor: an incremental fetch never sees a correction or a deletion.
[`examples/reconciled_backend.rs`](../examples/reconciled_backend.rs) replicates a small table
of spot measurements; the parts that differ from the append case are below.

Declare the source reconciled, and the driver asks for every stream with `since: None` on every
cycle:

```rust
/// The source is edited in place, so every cycle re-reads it whole and the driver asks
/// for it without a cursor.
fn reconciled(&self) -> bool {
    true
}
```

Send the full content with a completeness window. With a `SourceWindow` the request is a diff
rather than an append: the server classifies stored keys as new, changed, unchanged or
withdrawn, corrects changed values in place (flags and sample links untouched), and stamps a row
absent from the payload `withdrawn_at` (reversible: a later honest window re-asserting the row
clears it). Nothing is deleted.

```rust
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
```

The rules the server holds a window to:

- Only a stream declared `spot` accepts one, and only a sync service may send one. A windowed
  payload goes out as a single request whatever its size, because each chunk would claim the
  whole window with a fraction of it.
- `source_rows_read` and `dropped_times` are what make a withdrawal honest. An empty payload
  claiming rows were read, over stored content, is refused outright; a decode failure reported
  in `dropped_times` retains the stored row instead of withdrawing it. Withdrawal is computed as
  stored minus (admitted, rejected, dropped), so a funnel rejection never reads as a source
  deletion.
- **Curation wins.** A flagged, hand-curved or labelled reading never changes servedness without
  a person: its withdrawal is not stamped and a `source_modified` hold is raised; a corrected
  value on such a row is applied with the same hold.
- **The brake.** A pass that would change plus withdraw more than 15% of the window's stored
  rows (5-row floor), or lose one replicate index from more than half the groups, applies only
  its new rows and raises one `brake_fired` hold. Acknowledging that hold admits exactly one
  braked-scale pass; the source re-asserts the window every cycle, so nothing is lost in the
  meantime.
- Every diffed pass commits an `ingest_receipts` row (`GET /api/streams/{id}/receipts`), and the
  response echoes `accepted_window`. A missing echo means the API image predates reconciliation
  and silently ignored the claim; the client treats that as an error rather than degrading the
  source to append mode.
- **The digest handshake.** The driver digests each windowed payload's source-asserted content
  and skips the ingest when it matches `data_streams.last_window_digest`, which the server
  persists only after a clean pass (no brake, no holds, no rejections). Braked and held windows
  therefore keep re-asserting until a person rules. Server curation never enters the digest.
  The weekly `sync_full_reassert` schedule queues `trigger_full_sync` so drift the digests
  cannot see is still repaired.

## Status events and custom commands

Device telemetry travels as free-text status events on the same stream, through
`POST /api/ingest/status_events`. The server keeps a series' first value and its transitions: a
status equal to the stream's latest stored value inserts nothing, so polling every cycle does
not accrete "still the same" rows.

```rust
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
```

A command the runner does not know is forwarded to the backend with its payload. The `Ok`
value is stored as the command's result on the System page; an `Err` marks it failed with the
message.

```rust
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
```

## Worked example: river-data-vaisala

[river-data-sync-vaisala](https://github.com/RIVER-EPFL/river-data-sync-vaisala) reads the
viewLinc REST API. `main.rs` is a `run_sync_service` call that builds `VaisalaBackend` from
`VAISALA_BASE_URL`, `VAISALA_BEARER_TOKEN` and `VAISALA_SKIP_TLS_VERIFY` (a self-signed
certificate on the instrument network).

- `source_system` is `vaisala`; `source_key` is the location's `node_id`; `source_path` is the
  viewLinc hierarchy path, from which `metadata.hierarchy` (`project`, `site`, `parameter`) is
  derived so the pairing wizard can propose a site. Device serials, units and sample interval
  ride in `metadata.device` and siblings. Every stream is `continuous`.
- Append-only, so `reconciled()` stays false and discovery runs once at startup plus on full
  syncs. `fetch_readings` splits the requests into two upstream calls: streams with no cursor
  fetch back `MAX_HISTORY_DAYS` (90), the rest fetch from the earliest cursor in the group, so
  one new stream never drags the others back to the full window. Timestamps are snapped to the
  logger's 10-minute cadence, and a point with an unrepresentable epoch is skipped rather than
  stored at the present instant, which would latch the cursor.
- `fetch_status_events` rides the readings cycle but emits at most every
  `STATUS_INTERVAL_SECONDS` (1800): one `status=... battery=... signal=...` line per location.

## Worked example: river-data-rshiny

[river-data-rshiny](https://github.com/RIVER-EPFL/river-data-rshiny) is one binary for three
portals, chosen by `PORTAL_TYPE` (`cnet`, `metalp`, `nomis`) with `PORTAL_DB_*` connection
settings. CNET and METALP share a schema and `RshinyBackend`; NOMIS has its own model and
`NomisBackend`.

- `source_system` is the portal name; `source_key` is `station:column` for a single column and
  `station:mean_column:reps` for a replicate family. `rediscover_every_cycle()` is true because
  new stations and plotted columns appear without operator action and discovery is two cheap
  queries.
- The portals are mutable (the median CNET row is edited 280 days after its measurement) and
  hold under a thousand rows each, so `RshinyBackend` is `reconciled()`: every cycle re-reads
  each station's full history by keyset pagination and attaches a `SourceWindow` to every spot
  stream, with `source_rows_read` the station's row count and `dropped_times` the instants whose
  cells failed to decode (`PortalCell::Undecodable`). Continuous columns carry no window.
- Replicate families come from the portal's own `parameter_calculations` catalog: members,
  mean and sd columns, the curve-reference column and the R calculation name are declared on
  the `ReplicateSpec`, `sd_estimator` deliberately left `None`, and the pinned assignments from
  `apply_replicate_assignments` decide each member's `replicate_index`. The portal's mean and sd
  travel as `GroupAudit`s.
- Standard curves are discovered from the portal's curve table, registered through
  `discover_standard_curves`, and the mapped ids stamp `standard_curve_id` on the readings; the
  curve a portal applied to a corrected column travels as an annotation on that instant.
- NOMIS is append-only by design (`reconciled()` false) and declares every stream `spot`.

## Pairing and moving a source

Registration puts a stream on the Streams page; an administrator pairs it from there or with
the pairing wizard (`/api/sync/pairing-plans`, which reads `metadata.hierarchy` and
`metadata.coordinates`). The endpoints behind that, all requiring an administrator or a token
with `write_metadata`:

| Endpoint | Effect |
|----------|--------|
| `POST /api/streams/{id}/pair {site_parameter_id}` | Sets the pairing, backfills every stored reading with the site and parameter, re-derives each reading's curve from the window covering its time and refreshes aggregates as a tracked job |
| `POST /api/streams/{id}/unpair` | Clears the pairing and the readings' site and parameter (they leave every rollup) |
| `POST /api/streams/{id}/import` | Creates or reuses the instrument named by the stream's metadata serial and links it, without deploying it to a site |
| `POST /api/actions/merge_site_parameters {source_site_parameter_id, target_site_parameter_id}` | Absorbs one slot into another (readings, status events, samples, annotations and streams move; the source slot is deleted). Requires `manage_sensors`. Refused when the two slots hold samples at the same instant |

When an instrument moves from one collection system to another, the new service registers a
new stream (`("campbell", "CH1")` beside `("vaisala", "1270")`), and its readings arrive
unattributed. Pair the new stream to the same site parameter and the timeline continues: both
streams feed one slot, and the pairing history is the record of the move. If the new stream was
paired to a separate slot first, merge that slot into the original. Nothing about this needs
code in the service.

Sync events and receipts are the audit trail: `GET /api/sync/events` for cycles,
`GET /api/streams/{id}/receipts` for windowed passes, `GET /api/streams/{id}/stats` for counts
and the withdrawn tally.

## Retiring a service

`POST /api/sync/credentials/{id}/revoke` revokes one credential and ends its sessions; the
service's next heartbeat gets a 401, and its re-enrollment is refused until a new credential is
minted. `POST /api/sync/services/{id}/revoke` does the same for every credential of a service.
The streams and readings stay; unpair or merge them as above if another source takes over.

## Checklist

- An administrator has minted credentials for the service type.
- `source_system` is fixed and `source_key` is stable across restarts.
- Every descriptor declares `measurement_type`; a replicate family declares `replicates` and
  `spot`; a stream that will carry curve claims names its `sensor_id`.
- An append source filters by `req.since`; a mutable source declares `reconciled()` and attaches
  a `SourceWindow` with honest `source_rows_read` and `dropped_times`.
- The service enrolls against the dev API and shows on the System page with a completed cycle.
- Its streams are on the Streams page and an administrator has paired them.
