use crate::client::river_data_client::RiverDataClient;
use crate::models::SyncResult;

/// A sync service the runner can drive. Most services should implement
/// `SourceBackend` and use `SyncDriver` instead; this trait is the escape
/// hatch for services that need full control of the sync cycle.
#[async_trait::async_trait]
pub trait SyncService: Send + Sync + 'static {
    async fn sync(
        &self,
        full: bool,
    ) -> Result<SyncResult, Box<dyn std::error::Error + Send + Sync>>;

    async fn handle_command(
        &self,
        command: &str,
        _payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("Unknown command: {command}").into())
    }

    /// Client used for sync-event reporting and command acknowledgement.
    /// Returning None skips both silently.
    fn river_data_client(&self) -> Option<&RiverDataClient> {
        None
    }

    fn update_token(&self, token: &str) {
        if let Some(api) = self.river_data_client() {
            api.set_token(token);
        }
    }
}
