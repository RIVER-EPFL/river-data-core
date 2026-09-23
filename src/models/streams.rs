use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::replicates::{ColumnAssignment, ReplicateSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStream {
    pub id: Uuid,
    pub source_system: String,
    pub source_key: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub metadata: serde_json::Value,
    pub site_parameter_id: Option<Uuid>,
    /// Stream-level default for readings.measurement_type ('continuous' | 'spot' | 'derived').
    /// None defers to the API's sensor-frequency resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    pub is_active: bool,
    pub last_data_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Content digest of the last cleanly-applied windowed pass, as claimed by the sync client.
    /// Absent on APIs that predate the handshake; the client then sends full windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_window_digest: Option<String>,
    /// The authoritative replicate column-to-index mapping, present on the
    /// register and list responses for a stream declaring a replicate family.
    /// Absent on an API that predates it; the same list is then read out of
    /// `metadata.replicates`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<Vec<ColumnAssignment>>,
}

/// How many instruments a source's channels stand for, declared by the connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum InstrumentGranularity {
    /// One instrument per parameter across every site the source reports it at (a field portal).
    PerParameter,
    /// One instrument per site and parameter, stationed there (a logger network).
    PerSiteParameter,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisterStreamRequest {
    pub source_system: String,
    pub source_key: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub metadata: serde_json::Value,
    /// Stream-level classification declared at discovery. None never clears an operator-set value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    /// Owning sensor. Required for curve-carrying streams: the API admits a
    /// reading's curve claim only when reading-sensor == curve-sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    /// Replicate-family declaration; requires `measurement_type: "spot"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicates: Option<ReplicateSpec>,
    /// The source's decimal places for this channel (0 to 10). None declares nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimal_places: Option<i16>,
    /// The instrument this channel suggests at pairing. None leaves it to the API's inference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_granularity: Option<InstrumentGranularity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IngestReading {
    pub time: chrono::DateTime<chrono::Utc>,
    pub raw_value: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub replicate_index: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<Uuid>,
    /// Per-reading override ('continuous' | 'spot' | 'derived'). None resolves server-side from
    /// the stream default, then the owning sensor's data_frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_type: Option<String>,
    /// Standard curve the source applied to this reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_curve_id: Option<Uuid>,
}

impl IngestReading {
    /// A reading at replicate 0 with no sensor attribution; the server resolves the rest.
    pub fn new(time: chrono::DateTime<chrono::Utc>, raw_value: f64) -> Self {
        Self {
            time,
            raw_value,
            replicate_index: 0,
            sensor_id: None,
            calibration_id: None,
            deployment_id: None,
            measurement_type: None,
            standard_curve_id: None,
        }
    }
}

fn is_zero(v: &i16) -> bool {
    *v == 0
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct IngestStatusEvent {
    pub time: chrono::DateTime<chrono::Utc>,
    pub value: String,
    /// The instrument the status describes, when the source knows it. None leaves the event
    /// attributed to the stream alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_id: Option<Uuid>,
}

#[cfg(test)]
#[path = "tests/streams.rs"]
mod tests;
