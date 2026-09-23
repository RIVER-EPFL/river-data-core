use super::*;

#[test]
fn test_is_credentials_refused_auth_statuses() {
    assert!(is_credentials_refused(reqwest::StatusCode::UNAUTHORIZED));
    assert!(is_credentials_refused(reqwest::StatusCode::FORBIDDEN));
}

#[test]
fn test_is_credentials_refused_server_errors_are_not_revocation() {
    assert!(!is_credentials_refused(
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    ));
    assert!(!is_credentials_refused(
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    ));
}

#[test]
fn test_new_creates_client() {
    let client = ControlPlaneClient::new("http://localhost:3000").unwrap();
    assert_eq!(client.session_token(), None);
}

#[test]
fn test_base_url_strips_trailing_slash() {
    let client = ControlPlaneClient::new("http://localhost:3000/").unwrap();
    assert_eq!(
        client.service_url("/enroll"),
        "http://localhost:3000/api/sync/enroll"
    );
}

#[test]
fn test_service_url_construction() {
    let client = ControlPlaneClient::new("http://api:3000").unwrap();
    assert_eq!(
        client.service_url("/enroll"),
        "http://api:3000/api/sync/enroll"
    );
    assert_eq!(
        client.service_url("/heartbeat"),
        "http://api:3000/api/sync/heartbeat"
    );
}
