pub const TRIGGER_SYNC: &str = "trigger_sync";
pub const TRIGGER_FULL_SYNC: &str = "trigger_full_sync";
pub const PAUSE: &str = "pause";
pub const RESUME: &str = "resume";
/// Re-fetch named streams from the start of history and ingest with overwrite.
/// Payload: `{ "source_keys": [...], "overwrite": true }`.
pub const RESYNC_STREAMS: &str = "resync_streams";
