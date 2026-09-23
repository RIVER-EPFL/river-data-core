use serde::{Deserialize, Serialize};

/// The cadence vocabulary a reading is classified under. The wire fields stay strings, so a
/// producer names a value through this rather than spelling the literal and a consumer refuses
/// what it does not recognise instead of falling through to the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementType {
    Continuous,
    Spot,
    Derived,
}

impl MeasurementType {
    pub const ALL: &[MeasurementType] = &[Self::Continuous, Self::Spot, Self::Derived];

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Spot => "spot",
            Self::Derived => "derived",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

impl std::fmt::Display for MeasurementType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MeasurementType {
    type Err = super::UnknownValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
            .ok_or_else(|| super::UnknownValue::new(s, Self::ALL.iter().map(Self::as_str)))
    }
}

#[cfg(test)]
#[path = "tests/measurement.rs"]
mod tests;
