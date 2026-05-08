use sea_orm::DatabaseConnection;

pub trait SyncState: Clone + Send + Sync + 'static {
    fn db(&self) -> &DatabaseConnection;

    fn hash_token(&self, raw: &str) -> String {
        crate::crypto::hash_token(raw)
    }

    fn generate_token(&self) -> String {
        crate::crypto::generate_token()
    }
}
