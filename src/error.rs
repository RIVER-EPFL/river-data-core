#[cfg(feature = "client")]
#[derive(Debug, thiserror::Error)]
pub enum RiverDataClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
}

#[cfg(feature = "client")]
#[derive(Debug, thiserror::Error)]
pub enum ControlPlaneError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {status} from {url}: {body}")]
    Api {
        status: u16,
        url: String,
        body: String,
    },
    #[error("Enrollment failed: credentials revoked or invalid")]
    CredentialsRevoked,
}
