use axum::extract::State;
use axum::Json;
use chrono::Utc;
use moka::future::Cache;
use sea_orm::{ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter, Set, Statement, DatabaseBackend};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;
use uuid::Uuid;

use super::{SyncError, SyncResult};
use crate::server::entity::{sync_commands, sync_services};
use crate::server::handlers::enroll::create_session_token;
use crate::server::state::SyncState;

pub(crate) static SESSION_TOKEN_CACHE: LazyLock<Cache<Uuid, String>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(13 * 60))
        .build()
});

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub service_id: Uuid,
    pub status: String,
    pub current_operation: Option<String>,
}

#[derive(Serialize)]
pub struct HeartbeatResponse {
    pub session_token: String,
    pub pending_commands: Vec<PendingCommandResponse>,
}

#[derive(Serialize)]
pub struct PendingCommandResponse {
    pub id: Uuid,
    pub command: String,
    pub payload: Option<serde_json::Value>,
}

pub async fn heartbeat<S: SyncState>(
    State(state): State<S>,
    Json(req): Json<HeartbeatRequest>,
) -> SyncResult<Json<HeartbeatResponse>> {
    const VALID_STATUSES: &[&str] = &[
        "starting", "idle", "running", "paused", "syncing", "error", "stopping",
    ];
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(SyncError::BadRequest(format!(
            "Invalid status '{}'. Valid: {}",
            req.status,
            VALID_STATUSES.join(", ")
        )));
    }

    let service = sync_services::Entity::find_by_id(req.service_id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Service not found".to_string()))?;

    let mut active: sync_services::ActiveModel = service.into();
    active.status = Set(req.status);
    active.current_operation = Set(req.current_operation);
    active.last_heartbeat = Set(Some(Utc::now().into()));
    active.updated_at = Set(Utc::now().into());
    active.update(state.db()).await?;

    let session_token = if let Some(cached) = SESSION_TOKEN_CACHE.get(&req.service_id).await {
        cached
    } else {
        let token = create_session_token(&state, req.service_id).await?;
        SESSION_TOKEN_CACHE
            .insert(req.service_id, token.clone())
            .await;
        token
    };

    let pending = sync_commands::Entity::find()
        .filter(
            Condition::all()
                .add(sync_commands::Column::ServiceId.eq(req.service_id))
                .add(sync_commands::Column::Status.eq("pending"))
                .add(sync_commands::Column::ExpiresAt.gt(Utc::now())),
        )
        .all(state.db())
        .await?;

    let pending_commands = pending
        .into_iter()
        .map(|c| PendingCommandResponse {
            id: c.id,
            command: c.command,
            payload: c.payload,
        })
        .collect();

    let db_clone = state.db().clone();
    let sid = req.service_id;
    tokio::spawn(async move {
        let _ = db_clone
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE sync_commands SET status = 'expired' WHERE service_id = $1 AND status = 'pending' AND expires_at < NOW()",
                [sid.into()],
            ))
            .await;
    });

    Ok(Json(HeartbeatResponse {
        session_token,
        pending_commands,
    }))
}
