use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use supervisor_types::moderate::ModerationResponse;
use supervisor_types::partner::{
    ConfirmAuthorizationRequest, ConfirmAuthorizationResponse, PartnerCheckoutRequest,
    PartnerCheckoutResponse, PartnerModerationRequest, PartnerTokenRequest,
    PartnerTokenResponse, PartnerUserInfo, ProvisionUserRequest, ProvisionUserResponse,
    StripeConnectStatusResponse,
};
use tokio::sync::Mutex;

use crate::error::{Result, SupervisorError};

const DEFAULT_BASE_URL: &str = "https://api.supervisor.gg";

struct TokenState {
    access_token: String,
    expires_at: std::time::Instant,
}

/// Async client for the Supervisor Partner API with OAuth2 client credentials.
pub struct PartnerClient {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    base_url: String,
    token: Arc<Mutex<Option<TokenState>>>,
}

impl PartnerClient {
    /// Create a new partner client.
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self::with_base_url(client_id, client_secret, DEFAULT_BASE_URL)
    }

    /// Create a new partner client with a custom base URL.
    pub fn with_base_url(client_id: &str, client_secret: &str, base_url: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_token(&self) -> Result<String> {
        let mut token_guard = self.token.lock().await;

        if let Some(state) = token_guard.as_ref() {
            if std::time::Instant::now() < state.expires_at - std::time::Duration::from_secs(30) {
                return Ok(state.access_token.clone());
            }
        }

        let request = PartnerTokenRequest {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            grant_type: "client_credentials".to_string(),
        };

        let response = self
            .http
            .post(format!("{}/api/partner/token", self.base_url))
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

        let token_resp: PartnerTokenResponse = response.json().await?;
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
        self.request(reqwest::Method::POST, "/api/partner/users/provision", Some(&req))
            .await
    }

    /// List all users linked to this partner.
    pub async fn list_users(&self) -> Result<Vec<PartnerUserInfo>> {
        self.request::<Vec<PartnerUserInfo>>(reqwest::Method::GET, "/api/partner/users", None::<&()>.as_ref())
            .await
    }

    /// Get a specific linked user by ID.
    pub async fn get_user(&self, user_id: &str) -> Result<PartnerUserInfo> {
        self.request(
            reqwest::Method::GET,
            &format!("/api/partner/users/{}", user_id),
            None::<&()>.as_ref(),
        )
        .await
    }

    /// Moderate content on behalf of a linked user.
    pub async fn moderate(&self, request: PartnerModerationRequest) -> Result<ModerationResponse> {
        self.request(reqwest::Method::POST, "/api/partner/moderate", Some(&request))
            .await
    }

    /// Create a Stripe checkout session for a partner user.
    pub async fn create_checkout(
        &self,
        request: PartnerCheckoutRequest,
    ) -> Result<PartnerCheckoutResponse> {
        self.request(reqwest::Method::POST, "/api/partner/checkout", Some(&request))
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
            "/api/partner/users/confirm-authorization",
            Some(&req),
        )
        .await
    }

    /// Get the Stripe Connect onboarding status.
    pub async fn get_connect_status(&self) -> Result<StripeConnectStatusResponse> {
        self.request(reqwest::Method::GET, "/api/partner/connect/status", None::<&()>.as_ref())
            .await
    }
}
