use axum::routing::{get, patch, post};
use axum::Router;

use crate::server::handlers::{admin, commands, enroll, events, heartbeat};
use crate::server::state::SyncState;

/// Build the sync control plane routers.
///
/// Returns `(service_routes, admin_routes)`:
/// - `service_routes`: Mount at `/api/v1/sync` — enrollment (unauthenticated),
///   heartbeat + commands + events (auth via SyncServiceContext extractor)
/// - `admin_routes`: Mount at `/api/v1/sync` under admin middleware — the host app
///   should wrap with its own admin auth (e.g., Keycloak)
pub fn routes<S: SyncState>() -> (Router<S>, Router<S>) {
    let service_routes = Router::new()
        .route("/enroll", post(enroll::enroll::<S>))
        .route("/heartbeat", post(heartbeat::heartbeat::<S>))
        .route("/commands/{id}", patch(commands::update_command::<S>))
        .route("/events", post(events::create_sync_event::<S>))
        .route("/events/{id}", patch(events::update_sync_event::<S>));

    let admin_routes = Router::new()
        .route("/services", get(admin::list_services::<S>))
        .route("/services/{id}", get(admin::get_service::<S>))
        .route(
            "/services/{id}/commands",
            post(admin::issue_command::<S>),
        )
        .route(
            "/services/{id}/revoke",
            post(admin::revoke_service::<S>),
        )
        .route("/commands", get(admin::list_commands::<S>))
        .route("/events", get(admin::list_sync_events::<S>))
        .route(
            "/credentials",
            get(admin::list_credentials::<S>).post(admin::create_credential::<S>),
        )
        .route(
            "/credentials/{id}/revoke",
            post(admin::revoke_credential::<S>),
        );

    (service_routes, admin_routes)
}
