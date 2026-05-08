use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::server::entity::sync_service_tokens;
use crate::server::handlers::SyncError;
use crate::server::state::SyncState;

#[derive(Debug, Clone)]
pub struct SyncServiceContext {
    pub service_id: Uuid,
}

impl<S: SyncState> FromRequestParts<S> for SyncServiceContext {
    type Rejection = SyncError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let raw_token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|v| v.trim().to_string())
            .ok_or_else(|| {
                SyncError::Unauthorized("Bearer token required".to_string())
            })?;

        if raw_token.is_empty() {
            return Err(SyncError::Unauthorized(
                "Bearer token required".to_string(),
            ));
        }

        let token_hash = state.hash_token(&raw_token);

        let token = sync_service_tokens::Entity::find()
            .filter(sync_service_tokens::Column::TokenHash.eq(&token_hash))
            .one(state.db())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "DB error looking up sync token");
                SyncError::Unauthorized("Invalid session token".to_string())
            })?
            .ok_or_else(|| {
                tracing::debug!(token_hash_prefix = %&token_hash[..8], "Sync token not found");
                SyncError::Unauthorized("Invalid session token".to_string())
            })?;

        if token.expires_at.with_timezone(&chrono::Utc) < chrono::Utc::now() {
            tracing::debug!(service_id = %token.service_id, "Sync token expired");
            return Err(SyncError::Unauthorized(
                "Session token expired".to_string(),
            ));
        }

        Ok(SyncServiceContext {
            service_id: token.service_id,
        })
    }
}
