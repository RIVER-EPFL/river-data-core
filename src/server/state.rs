use sea_orm::DatabaseConnection;

use crate::models::SyncServerConfig;

pub trait SyncState: Clone + Send + Sync + 'static {
    fn db(&self) -> &DatabaseConnection;

    fn sync_config(&self) -> &SyncServerConfig {
        static DEFAULT: std::sync::LazyLock<SyncServerConfig> =
            std::sync::LazyLock::new(SyncServerConfig::default);
        &DEFAULT
    }

    fn hash_token(&self, raw: &str) -> String {
        crate::crypto::hash_token(raw)
    }

    fn generate_token(&self) -> String {
        crate::crypto::generate_token()
    }
}
