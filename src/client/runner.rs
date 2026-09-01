use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::client::control_plane::ControlPlaneClient;
use crate::client::service::SyncService;
use crate::commands;
use crate::error::ControlPlaneError;
use crate::models::{
    CommandStatus, PendingCommand, RunnerConfig, ServiceStatus, SyncEventCreate, SyncEventStatus,
    SyncEventType, SyncEventUpdate, SyncResult, SyncTrigger,
};

type SharedServiceId = Arc<RwLock<Uuid>>;
type ActiveEvent = Arc<RwLock<Option<Uuid>>>;

/// Resolves on SIGINT, and on SIGTERM where available (kubernetes pod
/// termination sends SIGTERM first).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = match tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        ) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "SIGTERM handler unavailable, listening for SIGINT only");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Shortest cadence an operator may set. A server-set value below it is a misconfiguration that
/// would hammer the source, so the floor applies rather than the number.
const MIN_SYNC_INTERVAL_SECS: u64 = 30;

/// The cadence to run at: the operator's, floored, or the service's own configuration when the
/// server sets none.
fn clamp_interval(server: Option<u64>, configured: u64) -> u64 {
    match server {
        Some(secs) => secs.max(MIN_SYNC_INTERVAL_SECS),
        None => configured,
    }
}

/// The two pieces of server-owned state the heartbeat reconciles into the running service.
struct ServerState {
    paused: watch::Sender<bool>,
    interval: watch::Sender<u64>,
}

pub struct SyncServiceRunner<S: SyncService> {
    service: Arc<S>,
    config: RunnerConfig,
}

impl<S: SyncService> SyncServiceRunner<S> {
    pub fn new(service: S, config: RunnerConfig) -> Self {
        Self {
            service: Arc::new(service),
            config,
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut client = ControlPlaneClient::new(&self.config.api_base_url)?;

        if self.service.river_data_client().is_none() {
            tracing::warn!(
                "SyncService::river_data_client() is None: command acknowledgements and sync event reporting are disabled"
            );
        }

        tracing::info!(
            client_id = %self.config.client_id,
            instance_id = %self.config.instance_id,
            "Enrolling with control plane"
        );

        let enroll_resp = loop {
            match client
                .enroll(
                    &self.config.client_id,
                    &self.config.client_secret,
                    &self.config.instance_id,
                )
                .await
            {
                Ok(resp) => break resp,
                Err(ControlPlaneError::CredentialsRevoked) => {
                    tracing::warn!(
                        retry_secs = self.config.enrollment_retry_secs,
                        "Credentials not found or invalid, retrying"
                    );
                    tokio::time::sleep(Duration::from_secs(self.config.enrollment_retry_secs))
                        .await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, retry_secs = self.config.enrollment_retry_secs, "Enrollment failed, retrying");
                    tokio::time::sleep(Duration::from_secs(self.config.enrollment_retry_secs))
                        .await;
                }
            }
        };

        let service_id: SharedServiceId = Arc::new(RwLock::new(enroll_resp.service_id));
        self.service.update_token(&enroll_resp.session_token);
        tracing::info!(service_id = %enroll_resp.service_id, "Enrolled successfully");

        // Seed from the server-persisted pause state so a restart cannot undo
        // an operator's pause.
        if enroll_resp.paused {
            tracing::info!("Service is paused server-side; scheduled syncs disabled until resumed");
        }
        let (pause_tx, pause_rx) = watch::channel(enroll_resp.paused);
        // The operator's cadence, when set, wins over the service's own configuration; the
        // heartbeat reconciles later changes without a restart.
        let (interval_tx, interval_rx) = watch::channel(clamp_interval(
            enroll_resp.sync_interval_secs,
            self.config.sync_interval_secs,
        ));
        let (sync_tx, sync_rx) = mpsc::channel::<SyncTrigger>(16);
        let (current_op_tx, current_op_rx) = watch::channel::<Option<String>>(None);
        let active_event: ActiveEvent = Arc::new(RwLock::new(None));

        let _ = sync_tx.send(SyncTrigger::Scheduled).await;
        tracing::info!("Queued initial sync after enrollment");

        let hb_service = self.service.clone();
        let hb_config = self.config.clone();
        let hb_sync_tx = sync_tx.clone();
        let hb_server_state = ServerState {
            paused: pause_tx.clone(),
            interval: interval_tx.clone(),
        };
        let hb_service_id = service_id.clone();

        let heartbeat_handle = tokio::spawn(async move {
            Self::heartbeat_loop(
                client,
                hb_service_id,
                hb_config,
                hb_service,
                hb_sync_tx,
                hb_server_state,
                current_op_rx,
            )
            .await;
        });

        let sync_service = self.service.clone();
        let sync_active_event = active_event.clone();
        let sync_handle = tokio::spawn(async move {
            Self::sync_loop(
                sync_service,
                service_id,
                interval_rx,
                pause_rx,
                sync_rx,
                current_op_tx,
                sync_active_event,
            )
            .await;
        });

        tokio::select! {
            _ = shutdown_signal() => {
                tracing::info!("Received shutdown signal");
                let in_flight = *active_event.read().expect("active_event lock poisoned");
                if let (Some(eid), Some(api)) = (in_flight, self.service.river_data_client()) {
                    let update = SyncEventUpdate {
                        status: Some(SyncEventStatus::Failed),
                        errors: vec!["Service shut down mid-sync".to_string()],
                        ..Default::default()
                    };
                    if let Err(e) = api.update_sync_event(eid, &update).await {
                        tracing::warn!(error = %e, "Failed to close in-flight sync event on shutdown");
                    }
                }
            }
            _ = heartbeat_handle => {
                tracing::error!("Heartbeat loop exited unexpectedly");
            }
            _ = sync_handle => {
                tracing::error!("Sync loop exited unexpectedly");
            }
        }

        Ok(())
    }

    async fn heartbeat_loop(
        mut client: ControlPlaneClient,
        service_id: SharedServiceId,
        config: RunnerConfig,
        service: Arc<S>,
        sync_tx: mpsc::Sender<SyncTrigger>,
        server_state: ServerState,
        current_op_rx: watch::Receiver<Option<String>>,
    ) {
        let ServerState {
            paused: pause_tx,
            interval: interval_tx,
        } = server_state;
        let mut interval =
            tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let is_paused = *pause_tx.borrow();
            let current_op = current_op_rx.borrow().clone();

            let status = if is_paused {
                ServiceStatus::Paused
            } else if current_op.is_some() {
                ServiceStatus::Syncing
            } else {
                ServiceStatus::Idle
            };

            let id = *service_id.read().expect("service_id lock poisoned");
            match client.heartbeat(id, status, current_op.as_deref()).await {
                Ok(resp) => {
                    service.update_token(&resp.session_token);

                    if *pause_tx.borrow() != resp.paused {
                        tracing::info!(paused = resp.paused, "Pause state reconciled from server");
                        let _ = pause_tx.send(resp.paused);
                    }

                    let wanted =
                        clamp_interval(resp.sync_interval_secs, config.sync_interval_secs);
                    if *interval_tx.borrow() != wanted {
                        tracing::info!(
                            sync_interval_secs = wanted,
                            "Sync cadence reconciled from server"
                        );
                        let _ = interval_tx.send(wanted);
                    }

                    for cmd in resp.pending_commands {
                        Self::handle_command(&service, cmd, &sync_tx, &pause_tx).await;
                    }
                }
                Err(ControlPlaneError::CredentialsRevoked) => {
                    tracing::error!("Credentials revoked, attempting re-enrollment");
                    match client
                        .enroll(
                            &config.client_id,
                            &config.client_secret,
                            &config.instance_id,
                        )
                        .await
                    {
                        Ok(resp) => {
                            *service_id.write().expect("service_id lock poisoned") =
                                resp.service_id;
                            service.update_token(&resp.session_token);
                            let _ = pause_tx.send(resp.paused);
                            let _ = interval_tx.send(clamp_interval(
                                resp.sync_interval_secs,
                                config.sync_interval_secs,
                            ));
                            tracing::info!(service_id = %resp.service_id, "Re-enrolled successfully");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Re-enrollment failed");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Heartbeat failed");
                }
            }
        }
    }

    async fn handle_command(
        service: &Arc<S>,
        cmd: PendingCommand,
        sync_tx: &mpsc::Sender<SyncTrigger>,
        pause_tx: &watch::Sender<bool>,
    ) {
        tracing::info!(command = %cmd.command, id = %cmd.id, "Received command");

        let api = service.river_data_client();
        if let Some(api) = api {
            let _ = api
                .update_command(cmd.id, CommandStatus::Acknowledged, None)
                .await;
        }

        match cmd.command.as_str() {
            commands::TRIGGER_SYNC => {
                let _ = sync_tx
                    .send(SyncTrigger::Command {
                        id: cmd.id,
                        full: false,
                    })
                    .await;
            }
            commands::TRIGGER_FULL_SYNC => {
                let _ = sync_tx
                    .send(SyncTrigger::Command {
                        id: cmd.id,
                        full: true,
                    })
                    .await;
            }
            commands::PAUSE => {
                let _ = pause_tx.send(true);
                if let Some(api) = api {
                    let _ = api
                        .update_command(
                            cmd.id,
                            CommandStatus::Completed,
                            Some(serde_json::json!({"paused": true})),
                        )
                        .await;
                }
            }
            commands::RESUME => {
                let _ = pause_tx.send(false);
                if let Some(api) = api {
                    let _ = api
                        .update_command(
                            cmd.id,
                            CommandStatus::Completed,
                            Some(serde_json::json!({"resumed": true})),
                        )
                        .await;
                }
            }
            other => {
                let outcome = service.handle_command(other, cmd.payload).await;
                if let Some(api) = api {
                    let _ = match outcome {
                        Ok(result) => {
                            api.update_command(cmd.id, CommandStatus::Completed, Some(result))
                                .await
                        }
                        Err(e) => {
                            api.update_command(
                                cmd.id,
                                CommandStatus::Failed,
                                Some(serde_json::json!({"error": e.to_string()})),
                            )
                            .await
                        }
                    };
                }
            }
        }
    }

    async fn sync_loop(
        service: Arc<S>,
        service_id: SharedServiceId,
        mut interval_rx: watch::Receiver<u64>,
        pause_rx: watch::Receiver<bool>,
        mut sync_rx: mpsc::Receiver<SyncTrigger>,
        current_op_tx: watch::Sender<Option<String>>,
        active_event: ActiveEvent,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(*interval_rx.borrow()));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;

        loop {
            let trigger = tokio::select! {
                _ = interval.tick() => SyncTrigger::Scheduled,
                Some(t) = sync_rx.recv() => t,
                // A cadence change restarts the clock rather than waiting out the old period,
                // so shortening it takes effect now and lengthening it cannot fire early.
                Ok(()) = interval_rx.changed() => {
                    let secs = *interval_rx.borrow();
                    tracing::info!(sync_interval_secs = secs, "Sync cadence changed");
                    interval = tokio::time::interval(Duration::from_secs(secs));
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    interval.tick().await;
                    continue;
                }
            };

            let (full, command_id) = match &trigger {
                SyncTrigger::Scheduled => (false, None),
                SyncTrigger::Command { id, full } => (*full, Some(*id)),
            };

            if *pause_rx.borrow() && command_id.is_none() {
                tracing::debug!("Sync paused, skipping scheduled sync");
                continue;
            }

            tracing::info!(full, "Starting sync cycle");
            let start = Instant::now();

            let event_type = match &trigger {
                SyncTrigger::Command { full: true, .. } => SyncEventType::FullSync,
                SyncTrigger::Command { full: false, .. } => SyncEventType::Triggered,
                SyncTrigger::Scheduled => SyncEventType::Scheduled,
            };

            let op_label = if full { "Full Sync" } else { "Syncing" };
            let _ = current_op_tx.send(Some(op_label.to_string()));

            let event_id = if let Some(api) = service.river_data_client() {
                let event = SyncEventCreate {
                    service_id: *service_id.read().expect("service_id lock poisoned"),
                    command_id,
                    event_type,
                    status: SyncEventStatus::Running,
                };
                match api.create_sync_event(&event).await {
                    Ok(ev) => Some(ev.id),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to create sync event");
                        None
                    }
                }
            } else {
                None
            };

            *active_event.write().expect("active_event lock poisoned") = event_id;

            let mut result = service.sync(full).await;

            *active_event.write().expect("active_event lock poisoned") = None;
            let _ = current_op_tx.send(None);

            if let Ok(r) = &mut result {
                r.full_sync = full;
                r.duration_ms = start.elapsed().as_millis() as u64;
            }

            if let (Some(eid), Some(api)) = (event_id, service.river_data_client()) {
                let update = match &result {
                    Ok(r) => SyncEventUpdate {
                        status: Some(if r.errors.is_empty() {
                            SyncEventStatus::Completed
                        } else {
                            SyncEventStatus::Partial
                        }),
                        readings_synced: Some(r.readings_synced),
                        readings_skipped: Some(r.readings_skipped),
                        status_events_synced: Some(r.status_events_synced),
                        errors: r.errors.clone(),
                        log: r.log.clone(),
                        duration_ms: Some(r.duration_ms),
                    },
                    Err(e) => SyncEventUpdate {
                        status: Some(SyncEventStatus::Failed),
                        errors: vec![e.to_string()],
                        duration_ms: Some(start.elapsed().as_millis() as u64),
                        ..Default::default()
                    },
                };
                let _ = api.update_sync_event(eid, &update).await;
            }

            if let (Some(cmd_id), Some(api)) = (command_id, service.river_data_client()) {
                let (cmd_status, result_json) = match &result {
                    Ok(r) => (
                        CommandStatus::Completed,
                        serde_json::json!({
                            "readings_synced": r.readings_synced,
                            "readings_skipped": r.readings_skipped,
                            "status_events_synced": r.status_events_synced,
                            "errors": r.errors,
                            "duration_ms": r.duration_ms,
                        }),
                    ),
                    Err(e) => (
                        CommandStatus::Failed,
                        serde_json::json!({
                            "error": e.to_string(),
                            "duration_ms": start.elapsed().as_millis() as u64,
                        }),
                    ),
                };
                if let Err(e) = api
                    .update_command(cmd_id, cmd_status, Some(result_json))
                    .await
                {
                    tracing::warn!(error = %e, "Failed to update command status");
                }
            }

            Self::log_outcome(&result);

            // A cycle that overruns the interval leaves a tick already due, which
            // would start the next cycle immediately. Reset so scheduled cycles
            // always begin a full interval after the previous one completed.
            interval.reset();
        }
    }

    fn log_outcome(result: &Result<SyncResult, Box<dyn std::error::Error + Send + Sync>>) {
        match result {
            Ok(r) => {
                tracing::info!(
                    readings = r.readings_synced,
                    status_events = r.status_events_synced,
                    full = r.full_sync,
                    duration_ms = r.duration_ms,
                    errors = r.errors.len(),
                    "Sync completed"
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "Sync failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MIN_SYNC_INTERVAL_SECS, clamp_interval};

    #[test]
    fn the_server_cadence_wins_but_never_below_the_floor() {
        assert_eq!(clamp_interval(None, 300), 300);
        assert_eq!(clamp_interval(Some(3600), 300), 3600);
        assert_eq!(clamp_interval(Some(1), 300), MIN_SYNC_INTERVAL_SECS);
    }
}
