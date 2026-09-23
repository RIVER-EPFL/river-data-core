use super::*;

#[test]
fn test_require_missing() {
    let err = require("RIVER_CORE_TEST_UNSET").unwrap_err();
    assert!(err.contains("RIVER_CORE_TEST_UNSET"));
}

#[test]
fn test_string_or_unset() {
    assert_eq!(string_or("RIVER_CORE_TEST_UNSET", "fallback"), "fallback");
}

#[test]
fn test_parse_from_garbage_falls_back() {
    assert_eq!(parse_from(Some("not-a-number".to_string()), 7u64), 7);
}

#[test]
fn test_parse_from_valid() {
    assert_eq!(parse_from(Some("12".to_string()), 7u64), 12);
    assert_eq!(parse_from::<i64>(None, 42), 42);
}

#[test]
fn test_bool_from() {
    assert!(bool_from(Some("1".to_string()), false));
    assert!(bool_from(Some("true".to_string()), false));
    assert!(!bool_from(Some("no".to_string()), true));
    assert!(bool_from(None, true));
}
