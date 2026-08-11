use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Starting,
    Idle,
    Running,
    Paused,
    Syncing,
    Error,
    Stopping,
}

impl ServiceStatus {
    pub const ALL: &[ServiceStatus] = &[
        Self::Starting, Self::Idle, Self::Running, Self::Paused,
        Self::Syncing, Self::Error, Self::Stopping,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Syncing => "syncing",
            Self::Error => "error",
            Self::Stopping => "stopping",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Pending,
    Acknowledged,
    Completed,
    Failed,
    Expired,
}

impl CommandStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Acknowledged => "acknowledged",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }
}

impl std::fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventType {
    Scheduled,
    Manual,
    Command,
    Triggered,
    FullSync,
}

impl SyncEventType {
    pub const ALL: &[SyncEventType] = &[
        Self::Scheduled, Self::Manual, Self::Command, Self::Triggered, Self::FullSync,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::Command => "command",
            Self::Triggered => "triggered",
            Self::FullSync => "full_sync",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventStatus {
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
}

impl SyncEventStatus {
    pub const ALL: &[SyncEventStatus] = &[
        Self::Running, Self::Completed, Self::Partial, Self::Failed, Self::Cancelled,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Partial | Self::Failed | Self::Cancelled)
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Completed | Self::Partial)
    }
}
