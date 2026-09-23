//! Environment variable helpers shared by sync services.

use std::str::FromStr;

/// Read a required variable, with a readable error naming the missing key.
pub fn require(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("Missing required env var: {key}"))
}

/// Read a string variable, falling back to a default when unset.
pub fn string_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Parse a variable into any FromStr type, falling back on unset or unparseable values.
pub fn parse_or<T: FromStr>(key: &str, default: T) -> T {
    parse_from(std::env::var(key).ok(), default)
}

/// Read a boolean variable ("true" or "1"), falling back to a default when unset.
pub fn bool_or(key: &str, default: bool) -> bool {
    bool_from(std::env::var(key).ok(), default)
}

fn parse_from<T: FromStr>(value: Option<String>, default: T) -> T {
    value.and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn bool_from(value: Option<String>, default: bool) -> bool {
    value.map(|v| v == "true" || v == "1").unwrap_or(default)
}

#[cfg(test)]
#[path = "tests/env.rs"]
mod tests;
