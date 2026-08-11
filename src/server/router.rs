use axum::routing::{patch, post};
use axum::Router;

use crate::server::handlers::{commands, enroll, events, heartbeat};
use crate::server::state::SyncState;

/// Build the sync control plane router: mount at `/api/sync` for enrollment
/// (unauthenticated) plus heartbeat, commands and events (auth via the
/// `SyncServiceContext` extractor).
///
/// The operator-facing side of the control plane is not routed here. Its handlers are
/// public in [`crate::server::handlers::admin`] so the host app can mount each one behind
/// its own authorization, which is what river-data-api does.
pub fn routes<S: SyncState>() -> Router<S> {
    Router::new()
        .route("/enroll", post(enroll::enroll::<S>))
        .route("/heartbeat", post(heartbeat::heartbeat::<S>))
        .route("/commands/{id}", patch(commands::update_command::<S>))
        .route("/events", post(events::create_sync_event::<S>))
        .route("/events/{id}", patch(events::update_sync_event::<S>))
}
