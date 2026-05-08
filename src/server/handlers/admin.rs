use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, sea_query::Expr};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{SyncError, SyncResult};
use crate::commands;
use crate::models::CommandStatus;
use crate::server::entity::{
    sync_commands, sync_events, sync_service_credentials, sync_service_tokens, sync_services,
};
use crate::server::state::SyncState;

// ============================================================================
// Response Types
// ============================================================================

#[derive(Serialize)]
pub struct SyncServiceResponse {
    pub id: Uuid,
    pub service_type: String,
    pub instance_id: String,
    pub status: String,
    pub current_operation: Option<String>,
    pub last_heartbeat: Option<String>,
    pub last_sync_completed_at: Option<String>,
    pub last_error: Option<String>,
    pub health: String,
    pub created_at: String,
    pub updated_at: String,
}

fn compute_health(
    last_heartbeat: Option<chrono::DateTime<chrono::FixedOffset>>,
    config: &crate::models::SyncServerConfig,
) -> String {
    match last_heartbeat {
        None => "unknown".to_string(),
        Some(hb) => {
            let age = Utc::now() - hb.with_timezone(&Utc);
            if age.num_seconds() < config.health_healthy_secs {
                "healthy".to_string()
            } else if age.num_seconds() < config.health_warning_secs {
                "warning".to_string()
            } else {
                "stale".to_string()
            }
        }
    }
}

fn service_to_response(s: sync_services::Model, config: &crate::models::SyncServerConfig) -> SyncServiceResponse {
    let health = compute_health(s.last_heartbeat, config);
    SyncServiceResponse {
        id: s.id,
        service_type: s.service_type,
        instance_id: s.instance_id,
        status: s.status,
        current_operation: s.current_operation,
        last_heartbeat: s.last_heartbeat.map(|t| t.to_rfc3339()),
        last_sync_completed_at: s.last_sync_completed_at.map(|t| t.to_rfc3339()),
        last_error: s.last_error,
        health,
        created_at: s.created_at.to_rfc3339(),
        updated_at: s.updated_at.to_rfc3339(),
    }
}

#[derive(Serialize)]
pub struct SyncCommandResponse {
    pub id: Uuid,
    pub service_id: Uuid,
    pub command: String,
    pub payload: Option<serde_json::Value>,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub created_at: String,
    pub expires_at: String,
    pub acknowledged_at: Option<String>,
    pub completed_at: Option<String>,
}

fn command_to_response(c: sync_commands::Model) -> SyncCommandResponse {
    SyncCommandResponse {
        id: c.id,
        service_id: c.service_id,
        command: c.command,
        payload: c.payload,
        status: c.status,
        result: c.result,
        created_at: c.created_at.to_rfc3339(),
        expires_at: c.expires_at.to_rfc3339(),
        acknowledged_at: c.acknowledged_at.map(|t| t.to_rfc3339()),
        completed_at: c.completed_at.map(|t| t.to_rfc3339()),
    }
}

#[derive(Deserialize)]
pub struct IssueCommandRequest {
    pub command: String,
    pub payload: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct CreateCredentialRequest {
    pub service_type: String,
}

#[derive(Serialize)]
pub struct CreateCredentialResponse {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize)]
pub struct CredentialResponse {
    pub id: Uuid,
    pub client_id: String,
    pub service_type: String,
    pub service_id: Option<Uuid>,
    pub revoked: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct SyncEventResponse {
    pub id: Uuid,
    pub service_id: Uuid,
    pub command_id: Option<Uuid>,
    pub event_type: String,
    pub status: String,
    pub readings_synced: i64,
    pub status_events_synced: i64,
    pub errors: Option<serde_json::Value>,
    pub log: Option<serde_json::Value>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
}

fn sync_event_to_response(e: sync_events::Model) -> SyncEventResponse {
    SyncEventResponse {
        id: e.id,
        service_id: e.service_id,
        command_id: e.command_id,
        event_type: e.event_type,
        status: e.status,
        readings_synced: e.readings_synced,
        status_events_synced: e.status_events_synced,
        errors: e.errors,
        log: e.log,
        started_at: e.started_at.to_rfc3339(),
        completed_at: e.completed_at.map(|t| t.to_rfc3339()),
        duration_ms: e.duration_ms,
    }
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    25
}

// ============================================================================
// Handlers
// ============================================================================

pub async fn list_services<S: SyncState>(
    State(state): State<S>,
) -> SyncResult<Json<Vec<SyncServiceResponse>>> {
    let services = sync_services::Entity::find()
        .order_by_desc(sync_services::Column::UpdatedAt)
        .all(state.db())
        .await?;

    let config = state.sync_config();
    Ok(Json(services.into_iter().map(|s| service_to_response(s, config)).collect()))
}

pub async fn get_service<S: SyncState>(
    State(state): State<S>,
    Path(id): Path<Uuid>,
) -> SyncResult<Json<SyncServiceResponse>> {
    let service = sync_services::Entity::find_by_id(id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Service not found".to_string()))?;

    Ok(Json(service_to_response(service, state.sync_config())))
}

pub async fn issue_command<S: SyncState>(
    State(state): State<S>,
    Path(service_id): Path<Uuid>,
    Json(req): Json<IssueCommandRequest>,
) -> SyncResult<Json<SyncCommandResponse>> {
    sync_services::Entity::find_by_id(service_id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Service not found".to_string()))?;

    let valid_commands = [
        commands::TRIGGER_SYNC,
        commands::TRIGGER_FULL_SYNC,
        commands::PAUSE,
        commands::RESUME,
    ];
    if !valid_commands.contains(&req.command.as_str()) {
        return Err(SyncError::BadRequest(format!(
            "Invalid command '{}'. Valid commands: {}",
            req.command,
            valid_commands.join(", ")
        )));
    }

    let expiry_secs = state.sync_config().command_expiry_secs as i64;
    let cmd = sync_commands::ActiveModel {
        id: Set(Uuid::new_v4()),
        service_id: Set(service_id),
        command: Set(req.command),
        payload: Set(req.payload),
        status: Set(CommandStatus::Pending.to_string()),
        result: Set(None),
        created_at: Set(Utc::now().into()),
        expires_at: Set((Utc::now() + chrono::Duration::seconds(expiry_secs)).into()),
        acknowledged_at: Set(None),
        completed_at: Set(None),
    };

    let inserted = cmd.insert(state.db()).await?;
    Ok(Json(command_to_response(inserted)))
}

pub async fn list_commands<S: SyncState>(
    State(state): State<S>,
    Query(params): Query<PaginationQuery>,
) -> SyncResult<(StatusCode, HeaderMap, Json<Vec<SyncCommandResponse>>)> {
    use sea_orm::PaginatorTrait;

    let per_page = params.per_page.min(100);
    let page = params.page.max(1) - 1;

    let paginator = sync_commands::Entity::find()
        .order_by_desc(sync_commands::Column::CreatedAt)
        .paginate(state.db(), per_page);

    let total = paginator.num_items().await?;
    let commands: Vec<SyncCommandResponse> = paginator
        .fetch_page(page)
        .await?
        .into_iter()
        .map(command_to_response)
        .collect();

    let mut headers = HeaderMap::new();
    let start = page * per_page;
    let end = start + commands.len() as u64;
    let range_value = if commands.is_empty() {
        format!("items */{total}")
    } else {
        format!("items {start}-{end}/{total}")
    };
    if let Ok(hv) = range_value.parse() {
        headers.insert("Content-Range", hv);
    }

    Ok((StatusCode::OK, headers, Json(commands)))
}

pub async fn create_credential<S: SyncState>(
    State(state): State<S>,
    Json(req): Json<CreateCredentialRequest>,
) -> SyncResult<Json<CreateCredentialResponse>> {
    let full_token = state.generate_token();
    let prefix = &state.sync_config().client_id_prefix;
    let client_id = format!("{prefix}{}", &full_token[..16]);
    let client_secret = state.generate_token();
    let secret_hash = state.hash_token(&client_secret);

    let cred = sync_service_credentials::ActiveModel {
        id: Set(Uuid::new_v4()),
        client_id: Set(client_id.clone()),
        client_secret_hash: Set(secret_hash),
        service_type: Set(req.service_type),
        service_id: Set(None),
        revoked: Set(false),
        created_at: Set(Utc::now().into()),
    };

    cred.insert(state.db()).await?;

    Ok(Json(CreateCredentialResponse {
        client_id,
        client_secret,
    }))
}

pub async fn list_credentials<S: SyncState>(
    State(state): State<S>,
) -> SyncResult<Json<Vec<CredentialResponse>>> {
    let creds = sync_service_credentials::Entity::find()
        .order_by_desc(sync_service_credentials::Column::CreatedAt)
        .all(state.db())
        .await?;

    Ok(Json(
        creds
            .into_iter()
            .map(|c| CredentialResponse {
                id: c.id,
                client_id: c.client_id,
                service_type: c.service_type,
                service_id: c.service_id,
                revoked: c.revoked,
                created_at: c.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn revoke_credential<S: SyncState>(
    State(state): State<S>,
    Path(id): Path<Uuid>,
) -> SyncResult<Json<serde_json::Value>> {
    let cred = sync_service_credentials::Entity::find_by_id(id)
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::NotFound("Credential not found".to_string()))?;

    let mut active: sync_service_credentials::ActiveModel = cred.clone().into();
    active.revoked = Set(true);
    active.update(state.db()).await?;

    if let Some(service_id) = cred.service_id {
        sync_service_tokens::Entity::delete_many()
            .filter(sync_service_tokens::Column::ServiceId.eq(service_id))
            .exec(state.db())
            .await?;
    }

    Ok(Json(serde_json::json!({"revoked": true})))
}

pub async fn list_sync_events<S: SyncState>(
    State(state): State<S>,
    Query(params): Query<PaginationQuery>,
) -> SyncResult<(StatusCode, HeaderMap, Json<Vec<SyncEventResponse>>)> {
    use sea_orm::PaginatorTrait;

    let per_page = params.per_page.min(100);
    let page = params.page.max(1) - 1;

    let paginator = sync_events::Entity::find()
        .order_by_desc(sync_events::Column::StartedAt)
        .paginate(state.db(), per_page);

    let total = paginator.num_items().await?;
    let events = paginator.fetch_page(page).await?;

    let response: Vec<SyncEventResponse> =
        events.into_iter().map(sync_event_to_response).collect();

    let mut headers = HeaderMap::new();
    let range_value = if response.is_empty() {
        format!("items */{total}")
    } else {
        let start = page * per_page;
        let end = start + response.len() as u64 - 1;
        format!("items {start}-{end}/{total}")
    };
    headers.insert("Content-Range", range_value.parse().unwrap());
    headers.insert(
        "Access-Control-Expose-Headers",
        "Content-Range".parse().unwrap(),
    );

    Ok((StatusCode::OK, headers, Json(response)))
}

pub async fn revoke_service<S: SyncState>(
    State(state): State<S>,
    Path(id): Path<Uuid>,
) -> SyncResult<Json<serde_json::Value>> {
    sync_service_credentials::Entity::update_many()
        .col_expr(
            sync_service_credentials::Column::Revoked,
            Expr::value(true),
        )
        .filter(sync_service_credentials::Column::ServiceId.eq(id))
        .exec(state.db())
        .await?;

    sync_service_tokens::Entity::delete_many()
        .filter(sync_service_tokens::Column::ServiceId.eq(id))
        .exec(state.db())
        .await?;

    Ok(Json(serde_json::json!({"revoked": true})))
}
