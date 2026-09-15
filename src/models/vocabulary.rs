/// A string that is not one of a closed vocabulary's values.
///
/// Carries what was read and what was expected, because the two together are what a caller needs to
/// correct a stored value or a request field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{value}' is not one of: {expected}")]
pub struct UnknownValue {
    pub value: String,
    pub expected: String,
}

impl UnknownValue {
    pub(crate) fn new(value: &str, expected: impl Iterator<Item = &'static str>) -> Self {
        Self {
            value: value.to_string(),
            expected: expected.collect::<Vec<_>>().join(", "),
        }
    }
}
