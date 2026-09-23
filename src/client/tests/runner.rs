use super::{MIN_SYNC_INTERVAL_SECS, clamp_interval};

#[test]
fn the_server_cadence_wins_but_never_below_the_floor() {
    assert_eq!(clamp_interval(None, 300), 300);
    assert_eq!(clamp_interval(Some(3600), 300), 3600);
    assert_eq!(clamp_interval(Some(1), 300), MIN_SYNC_INTERVAL_SECS);
}
