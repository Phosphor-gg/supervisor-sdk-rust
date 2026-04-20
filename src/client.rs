use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use supervisor_types::moderate::{
    BatchModerationRequest, ModerationLabel, ModerationRequest, ModerationResponse,
    UsernameCheckRequest, UsernameCheckResponse,
};

use crate::error::{Result, SupervisorError};

const DEFAULT_BASE_URL: &str = "https://api.supervisor.gg";

/// Async client for the Supervisor content moderation API.
pub struct SupervisorClient {
    http: reqwest::Client,
    base_url: String,
}

impl SupervisorClient {
    /// Create a new client with the given API key.
    pub fn new(api_key: &str) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create a new client with a custom base URL.
    pub fn with_base_url(api_key: &str, base_url: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&impl serde::Serialize>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url);

        if let Some(body) = body {
            req = req.json(body);
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Err(SupervisorError::Api {
                status_code,
                message: body["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
                details: body["details"].as_str().map(String::from),
            });
        }

        Ok(response.json().await?)
    }

    /// Moderate text or an image for harmful content.
    pub async fn moderate(&self, request: ModerationRequest) -> Result<ModerationResponse> {
        self.request(reqwest::Method::POST, "/api/moderate", Some(&request))
            .await
    }

    /// Moderate multiple texts in a single request.
    pub async fn moderate_batch(
        &self,
        request: BatchModerationRequest,
    ) -> Result<Vec<ModerationResponse>> {
        self.request(reqwest::Method::POST, "/api/batch", Some(&request))
            .await
    }

    /// Check a username for policy violations.
    pub async fn check_username(&self, username: &str) -> Result<UsernameCheckResponse> {
        let request = UsernameCheckRequest {
            username: username.to_string(),
        };
        self.request(reqwest::Method::POST, "/api/username", Some(&request))
            .await
    }

    /// Get all available moderation labels.
    pub async fn get_labels(&self) -> Result<Vec<ModerationLabel>> {
        self.request::<Vec<ModerationLabel>>(reqwest::Method::GET, "/api/labels", None::<&()>.as_ref())
            .await
    }
}
