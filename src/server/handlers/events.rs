use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::Deserialize;
use uuid::Uuid;

use super::{SyncError, SyncResult};
use crate::server::entity::{sync_events, sync_services};
use crate::server::middleware::SyncServiceContext;
use crate::server::state::SyncState;

#[derive(Deserialize)]
pub struct CreateSyncEventRequest {
    pub service_id: Uuid,
    pub command_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateSyncEventRequest {
    pub status: Option<String>,
    pub readings_synced: Option<i64>,
    pub status_events_synced: Option<i64>,
    pub errors: Option<serde_json::Value>,
    pub log: Option<serde_json::Value>,
    pub duration_ms: Option<i64>,
}

pub async fn create_sync_event<S: SyncState>(
    State(state): State<S>,
    ctx: SyncServiceContext,
    Json(req): Json<CreateSyncEventRequest>,
) -> SyncResult<Json<serde_json::Value>> {
    if req.service_id != ctx.service_id {
        return Err(SyncError::Forbidden(
            "Event service_id does not match authenticated service".to_string(),
        ));
    }

    const VALID_EVENT_TYPES: &[&str] =
        &["scheduled", "manual", "command", "triggered", "full_sync"];
    const VALID_EVENT_STATUSES: &[&str] =
        &["running", "completed", "partial", "failed", "cancelled"];

    if let Some(ref event_type) = req.event_type
        && !VALID_EVENT_TYPES.contains(&event_type.as_str()) {
            return Err(SyncError::BadRequest(format!(
                "Invalid event_type '{}'. Valid: {}",
                event_type,
                VALID_EVENT_TYPES.join(", ")
            )));
        }

    if let Some(ref status) = req.status
        && !VALID_EVENT_STATUSES.contains(&status.as_str()) {
            return Err(SyncError::BadRequest(format!(
                "Invalid status '{}'. Valid: {}",
                status,
                VALID_EVENT_STATUSES.join(", ")
            )));
        }

    let event = sync_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        service_id: Set(req.service_id),
        command_id: Set(req.command_id),
        event_type: Set(req.event_type.unwrap_or_else(|| "scheduled".to_string())),
        status: Set(req.status.unwrap_or_else(|| "running".to_string())),
        readings_synced: Set(0),
        status_events_synced: Set(0),
        errors: Set(None),
        log: Set(None),
        started_at: Set(Utc::now().into()),
        completed_at: Set(None),
        duration_ms: Set(None),
    };

    let inserted = event.insert(state.db()).await?;

    Ok(Json(serde_json::json!({
        "id": inserted.id.to_string(),
        "service_id": inserted.service_id,
        "status": inserted.status,
    })))
}

pub async fn update_sync_event<S: SyncState>(
    State(state): State<S>,
    ctx: SyncServiceContext,
    Path(event_id): Path<Uuid>,
    Json(req): Json<UpdateSyncEventRequest>,
) -> SyncResult<Json<serde_json::Value>> {
    let event = sync_events::Entity::find_by_id(event_id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Sync event not found".to_string()))?;

    if event.service_id != ctx.service_id {
        return Err(SyncError::Forbidden(
            "Event does not belong to this service".to_string(),
        ));
    }

    let service_id = event.service_id;
    let mut active: sync_events::ActiveModel = event.into();

    let is_terminal = matches!(
        req.status.as_deref(),
        Some("completed" | "partial" | "failed" | "cancelled")
    );
    let is_success = matches!(req.status.as_deref(), Some("completed" | "partial"));

    if let Some(status) = req.status {
        active.status = Set(status);
    }
    if let Some(readings) = req.readings_synced {
        active.readings_synced = Set(readings);
    }
    if let Some(status_events) = req.status_events_synced {
        active.status_events_synced = Set(status_events);
    }
    if let Some(errors) = req.errors {
        active.errors = Set(Some(errors));
    }
    if let Some(log) = req.log {
        active.log = Set(Some(log));
    }
    if let Some(duration) = req.duration_ms {
        active.duration_ms = Set(Some(duration));
    }
    if is_terminal {
        active.completed_at = Set(Some(Utc::now().into()));
    }

    active.update(state.db()).await?;

    if is_success
        && let Some(service) = sync_services::Entity::find_by_id(service_id)
            .one(state.db())
            .await?
        {
            let mut svc_active: sync_services::ActiveModel = service.into();
            svc_active.last_sync_completed_at = Set(Some(Utc::now().into()));
            svc_active.updated_at = Set(Utc::now().into());
            svc_active.update(state.db()).await?;
        }

    Ok(Json(serde_json::json!({"updated": true})))
}
