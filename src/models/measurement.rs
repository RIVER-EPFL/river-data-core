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

    pub fn from_str(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.as_str() == s).copied()
    }
}

impl std::fmt::Display for MeasurementType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measurement_type_round_trips_every_member() {
        for v in MeasurementType::ALL {
            assert_eq!(MeasurementType::from_str(v.as_str()), Some(*v));
        }
    }

    #[test]
    fn test_measurement_type_refuses_what_it_does_not_know() {
        // The typo the classification chain would otherwise resolve to its default.
        assert_eq!(MeasurementType::from_str("spott"), None);
        assert_eq!(MeasurementType::from_str("Spot"), None);
        assert_eq!(MeasurementType::from_str(""), None);
    }

    #[test]
    fn test_measurement_type_serialises_as_the_stored_string() {
        assert_eq!(
            serde_json::to_value(MeasurementType::Continuous).unwrap(),
            serde_json::json!("continuous")
        );
        assert_eq!(MeasurementType::Spot.to_string(), "spot");
    }
}
