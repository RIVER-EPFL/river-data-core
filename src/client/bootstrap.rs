use crate::client::backend::SourceBackend;
use crate::client::driver::SyncDriver;
use crate::client::river_data_client::RiverDataClient;
use crate::client::runner::SyncServiceRunner;
use crate::models::RunnerConfig;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Runs a sync service to completion: .env loading, tracing, config, enrollment
/// and the sync loop. The closure builds the source backend (DB pools, HTTP
/// clients, source config). Blocks until shutdown; call it from a plain `main`.
pub fn run_sync_service<F, Fut>(build: F) -> Result<(), BoxError>
where
    F: FnOnce(RunnerConfig) -> Fut,
    Fut: Future<Output = Result<Box<dyn SourceBackend>, BoxError>>,
{
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = RunnerConfig::from_env()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let api = RiverDataClient::new(&config.api_base_url, "")?;
        let backend = build(config.clone()).await?;
        let driver = SyncDriver::new(backend, api, &config);
        SyncServiceRunner::new(driver, config).run().await
    })
}
