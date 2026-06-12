use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use supervisor_types::moderate::ModerationResponse;
use supervisor_types::platform::{
    ConfirmAuthorizationRequest, ConfirmAuthorizationResponse, PlatformCheckoutRequest,
    PlatformCheckoutResponse, PlatformModerationRequest, PlatformTokenRequest,
    PlatformTokenResponse, PlatformUserInfo, ProvisionUserRequest, ProvisionUserResponse,
    StripeConnectStatusResponse,
};
use tokio::sync::Mutex;

use crate::error::{Result, SupervisorError};

const DEFAULT_BASE_URL: &str = "https://api.supervisor.gg";

struct TokenState {
    access_token: String,
    expires_at: std::time::Instant,
}

/// Async client for the Supervisor Platform API with OAuth2 client credentials.
pub struct PlatformClient {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    base_url: String,
    token: Arc<Mutex<Option<TokenState>>>,
}

impl PlatformClient {
    /// Create a new platform client.
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new(client_id: &str, client_secret: &str) -> Result<Self> {
        Self::with_base_url(client_id, client_secret, DEFAULT_BASE_URL)
    }

    /// Create a new platform client with a custom base URL.
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn with_base_url(client_id: &str, client_secret: &str, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(Mutex::new(None)),
        })
    }

    async fn ensure_token(&self) -> Result<String> {
        let mut token_guard = self.token.lock().await;

        if let Some(state) = token_guard.as_ref() {
            if std::time::Instant::now() < state.expires_at - std::time::Duration::from_secs(30) {
                return Ok(state.access_token.clone());
            }
        }

        let request = PlatformTokenRequest {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            grant_type: "client_credentials".to_string(),
        };

        let response = self
            .http
            .post(format!("{}/api/platform/token", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Err(SupervisorError::Api {
                status_code,
                message: body["error"]
                    .as_str()
                    .unwrap_or("Token exchange failed")
                    .to_string(),
                details: body["details"].as_str().map(String::from),
            });
        }

        let token_resp: PlatformTokenResponse = response.json().await?;
        let access_token = token_resp.access_token.clone();

        *token_guard = Some(TokenState {
            access_token: token_resp.access_token,
            expires_at: std::time::Instant::now()
                + std::time::Duration::from_secs(token_resp.expires_in),
        });

        Ok(access_token)
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&impl serde::Serialize>,
    ) -> Result<T> {
        let token = self.ensure_token().await?;
        let url = format!("{}{}", self.base_url, path);

        let mut req = self.http.request(method, &url);
        req = req.header(AUTHORIZATION, format!("Bearer {}", token));

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

    /// Provision or link a user by email.
    pub async fn provision_user(&self, email: &str) -> Result<ProvisionUserResponse> {
        let req = ProvisionUserRequest {
            email: email.to_string(),
        };
        self.request(reqwest::Method::POST, "/api/platform/users/provision", Some(&req))
            .await
    }

    /// List all users linked to this platform.
    pub async fn list_users(&self) -> Result<Vec<PlatformUserInfo>> {
        self.request::<Vec<PlatformUserInfo>>(reqwest::Method::GET, "/api/platform/users", None::<&()>.as_ref())
            .await
    }

    /// Get a specific linked user by ID.
    pub async fn get_user(&self, user_id: &str) -> Result<PlatformUserInfo> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/platform/users/{}", user_id),
            None::<&()>.as_ref(),
        )
        .await
    }

    /// Moderate content on behalf of a linked user.
    pub async fn moderate(&self, request: PlatformModerationRequest) -> Result<ModerationResponse> {
        self.request(reqwest::Method::POST, "/api/platform/moderate", Some(&request))
            .await
    }

    /// Create a Stripe checkout session for a platform user.
    pub async fn create_checkout(
        &self,
        request: PlatformCheckoutRequest,
    ) -> Result<PlatformCheckoutResponse> {
        self.request(reqwest::Method::POST, "/api/platform/checkout", Some(&request))
            .await
    }

    /// Confirm a user's authorization with the provided code.
    pub async fn confirm_authorization(
        &self,
        code: &str,
    ) -> Result<ConfirmAuthorizationResponse> {
        let req = ConfirmAuthorizationRequest {
            code: code.to_string(),
        };
        self.request(
            reqwest::Method::POST,
            "/api/platform/users/confirm-authorization",
            Some(&req),
        )
        .await
    }

    /// Get the Stripe Connect onboarding status.
    pub async fn get_connect_status(&self) -> Result<StripeConnectStatusResponse> {
        self.request(reqwest::Method::GET, "/api/platform/connect/status", None::<&()>.as_ref())
            .await
    }
}
