use axum::extract::State;
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use super::{SyncError, SyncResult};
use crate::models::{EnrollRequest, EnrollResponse, ServiceStatus};
use crate::server::entity::{sync_service_credentials, sync_service_tokens, sync_services};
use crate::server::handlers::heartbeat::SESSION_TOKEN_CACHE;
use crate::server::state::SyncState;

pub(crate) async fn create_session_token<S: SyncState>(
    state: &S,
    service_id: Uuid,
) -> SyncResult<String> {
    let raw_token = state.generate_token();
    let token_hash = state.hash_token(&raw_token);
    let ttl_secs = state.sync_config().session_token_ttl_secs as i64;

    let token = sync_service_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        service_id: Set(service_id),
        token_hash: Set(token_hash.clone()),
        expires_at: Set((Utc::now() + chrono::Duration::seconds(ttl_secs)).into()),
        created_at: Set(Utc::now().into()),
    };
    token.insert(state.db()).await?;
    tracing::debug!(%service_id, token_hash_prefix = %&token_hash[..8], "Session token created");

    let db_clone = state.db().clone();
    tokio::spawn(async move {
        let _ = sync_service_tokens::Entity::delete_many()
            .filter(sync_service_tokens::Column::ServiceId.eq(service_id))
            .filter(sync_service_tokens::Column::ExpiresAt.lt(Utc::now()))
            .exec(&db_clone)
            .await;
    });

    Ok(raw_token)
}

/// Enroll a sync service instance with credentials. Validates `client_id`/`client_secret`
/// against `sync_service_credentials`, registers or updates a `sync_services` row keyed
/// by `(service_type, instance_id)`, and returns a session token used for subsequent
/// authenticated requests (heartbeat, command updates, events). Unauthenticated.
#[utoipa::path(
    post,
    path = "/enroll",
    request_body = EnrollRequest,
    responses(
        (status = 200, description = "Service enrolled; session token returned", body = EnrollResponse),
        (status = 401, description = "Invalid client_id, client_secret, or credentials revoked"),
    ),
    tag = "sync"
)]
pub async fn enroll<S: SyncState>(
    State(state): State<S>,
    Json(req): Json<EnrollRequest>,
) -> SyncResult<Json<EnrollResponse>> {
    let cred = sync_service_credentials::Entity::find()
        .filter(sync_service_credentials::Column::ClientId.eq(&req.client_id))
        .one(state.db())
        .await?
        .ok_or_else(|| SyncError::Unauthorized("Invalid client_id".to_string()))?;

    if cred.revoked {
        return Err(SyncError::Unauthorized(
            "Credentials have been revoked".to_string(),
        ));
    }

    let secret_hash = state.hash_token(&req.client_secret);
    if secret_hash != cred.client_secret_hash {
        return Err(SyncError::Unauthorized(
            "Invalid client_secret".to_string(),
        ));
    }

    let existing = sync_services::Entity::find()
        .filter(
            Condition::all()
                .add(sync_services::Column::ServiceType.eq(&cred.service_type))
                .add(sync_services::Column::InstanceId.eq(&req.instance_id)),
        )
        .one(state.db())
        .await?;

    let starting = ServiceStatus::Starting.to_string();

    let service_id = if let Some(existing) = existing {
        let mut active: sync_services::ActiveModel = existing.clone().into();
        active.status = Set(starting);
        active.current_operation = Set(None);
        active.last_error = Set(None);
        active.updated_at = Set(Utc::now().into());
        active.update(state.db()).await?;
        existing.id
    } else {
        let service = sync_services::ActiveModel {
            id: Set(Uuid::new_v4()),
            service_type: Set(cred.service_type.clone()),
            instance_id: Set(req.instance_id.clone()),
            status: Set(starting),
            current_operation: Set(None),
            last_heartbeat: Set(None),
            last_sync_completed_at: Set(None),
            last_error: Set(None),
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
        };
        let inserted = service.insert(state.db()).await?;
        inserted.id
    };

    if cred.service_id.is_none() {
        let mut cred_active: sync_service_credentials::ActiveModel = cred.into();
        cred_active.service_id = Set(Some(service_id));
        cred_active.update(state.db()).await?;
    }

    let session_token = create_session_token(&state, service_id).await?;
    SESSION_TOKEN_CACHE
        .insert(service_id, session_token.clone())
        .await;

    Ok(Json(EnrollResponse {
        service_id,
        session_token,
    }))
}
