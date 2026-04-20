use thiserror::Error;

/// Errors returned by the Supervisor SDK.
#[derive(Debug, Error)]
pub enum SupervisorError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned an error response.
    #[error("[{status_code}] {message}")]
    Api {
        status_code: u16,
        message: String,
        details: Option<String>,
    },

    /// Failed to serialize/deserialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl SupervisorError {
    /// Returns true if this is a 401 authentication error.
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::Api { status_code: 401, .. })
    }

    /// Returns true if this is a 429 rate limit error.
    pub fn is_rate_limit(&self) -> bool {
        matches!(self, Self::Api { status_code: 429, .. })
    }

    /// Returns the HTTP status code if this is an API error.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Api { status_code, .. } => Some(*status_code),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, SupervisorError>;
