use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

use super::{SyncError, SyncResult};
use crate::models::{CommandStatus, CommandUpdateRequest};
use crate::server::entity::sync_commands;
use crate::server::middleware::SyncServiceContext;
use crate::server::state::SyncState;

const VALID_UPDATE_STATUSES: &[&str] = &[
    CommandStatus::Acknowledged.as_str(),
    CommandStatus::Completed.as_str(),
    CommandStatus::Failed.as_str(),
];

/// Sync service reports the lifecycle status of a command it received via heartbeat.
/// Valid status transitions: `acknowledged` (in progress), `completed` (success with
/// optional result payload), `failed` (with error result). Only the owning service can
/// update its commands. Requires sync session token auth.
#[utoipa::path(
    patch,
    path = "/commands/{id}",
    params(("id" = Uuid, Path, description = "Sync command UUID")),
    request_body = CommandUpdateRequest,
    responses(
        (status = 200, description = "Command updated"),
        (status = 400, description = "Invalid status value"),
        (status = 401, description = "Invalid session token"),
        (status = 403, description = "Command belongs to a different service"),
        (status = 404, description = "Command not found"),
    ),
    tag = "sync"
)]
pub async fn update_command<S: SyncState>(
    State(state): State<S>,
    ctx: SyncServiceContext,
    Path(command_id): Path<Uuid>,
    Json(req): Json<CommandUpdateRequest>,
) -> SyncResult<Json<serde_json::Value>> {
    let cmd = sync_commands::Entity::find_by_id(command_id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Command not found".to_string()))?;

    if cmd.service_id != ctx.service_id {
        return Err(SyncError::Forbidden(
            "Command does not belong to this service".to_string(),
        ));
    }

    if !VALID_UPDATE_STATUSES.contains(&req.status.as_str()) {
        return Err(SyncError::BadRequest(format!(
            "Invalid status '{}'. Valid: {}",
            req.status,
            VALID_UPDATE_STATUSES.join(", ")
        )));
    }

    let mut active: sync_commands::ActiveModel = cmd.into();
    active.status = Set(req.status.clone());
    if req.result.is_some() {
        active.result = Set(req.result);
    }
    if req.status == CommandStatus::Acknowledged.as_str() {
        active.acknowledged_at = Set(Some(Utc::now().into()));
    }
    if req.status == CommandStatus::Completed.as_str()
        || req.status == CommandStatus::Failed.as_str()
    {
        active.completed_at = Set(Some(Utc::now().into()));
    }
    active.update(state.db()).await?;

    Ok(Json(serde_json::json!({"updated": true})))
}
