use std::collections::HashMap;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use base64::Engine as _;
use serde::Serialize;
use supervisor_types::moderate::{
    MAX_VIDEO_BYTES, ModerationLabel, VideoModerationRequest, VideoModerationResponse,
    ModerationModel, ModerationRequest, ModerationResponse, UsernameCheckRequest,
    UsernameCheckResponse,
};

use crate::error::{Result, SupervisorError};

const DEFAULT_BASE_URL: &str = "https://supervisor.gg";

/// Request body for the `/api/batch` endpoint.
///
/// If both `texts` and `images` are non-empty, their lengths must be equal;
/// this is validated client-side by [`SupervisorClient::moderate_batch`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchModerationRequest {
    /// Texts to moderate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<String>,
    /// Base64-encoded images to moderate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Moderation model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModerationModel>,
    /// Restrict moderation to a subset of labels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_labels: Option<Vec<supervisor_types::moderate::ModerationLabel>>,
    /// Whether to include contextual analysis in the response.
    #[serde(default)]
    pub include_context: bool,
}

/// Async client for the Supervisor content moderation API.
pub struct SupervisorClient {
    http: reqwest::Client,
    base_url: String,
}

impl SupervisorClient {
    /// Create a new client with the given API key.
    ///
    /// Returns an error if the API key contains characters invalid for an HTTP
    /// header, or if the underlying HTTP client cannot be built.
    pub fn new(api_key: &str) -> Result<Self> {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Create a new client with a custom base URL.
    ///
    /// Returns an error if the API key contains characters invalid for an HTTP
    /// header, or if the underlying HTTP client cannot be built.
    pub fn with_base_url(api_key: &str, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let auth = HeaderValue::from_str(&format!("Bearer {}", api_key)).map_err(|_| {
            SupervisorError::Validation(
                "invalid API key: contains characters not allowed in an HTTP header".to_string(),
            )
        })?;
        headers.insert(AUTHORIZATION, auth);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
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
    ///
    /// Any image is preprocessed client-side (downscaled and re-encoded as
    /// JPEG) before upload; see [`crate::prepare_image`].
    pub async fn moderate(&self, mut request: ModerationRequest) -> Result<ModerationResponse> {
        if let Some(image) = request.image.as_deref() {
            request.image = Some(crate::image_prep::prepare_image(image));
        }
        self.request(reqwest::Method::POST, "/api/moderate", Some(&request))
            .await
    }

    /// Moderate multiple texts and/or images in a single request.
    ///
    /// If both `texts` and `images` are non-empty, their lengths must match,
    /// otherwise a [`SupervisorError::Validation`] is returned before sending.
    ///
    /// Images are preprocessed client-side (downscaled and re-encoded as
    /// JPEG) before upload; see [`crate::prepare_image`].
    pub async fn moderate_batch(
        &self,
        mut request: BatchModerationRequest,
    ) -> Result<Vec<ModerationResponse>> {
        if !request.texts.is_empty()
            && !request.images.is_empty()
            && request.texts.len() != request.images.len()
        {
            return Err(SupervisorError::Validation(format!(
                "texts and images must have equal lengths when both are provided (got {} texts, {} images)",
                request.texts.len(),
                request.images.len()
            )));
        }
        request.images = request
            .images
            .iter()
            .map(|image| crate::image_prep::prepare_image(image))
            .collect();
        self.request(reqwest::Method::POST, "/api/batch", Some(&request))
            .await
    }

    /// Moderate a short video.
    ///
    /// Sends the clip to the API, which extracts the frames that actually
    /// differ (scene cuts rather than every frame) and moderates those, so a
    /// clip costs a handful of frames rather than hundreds.
    ///
    /// Requires the video-moderation entitlement on the account.
    ///
    /// The size limit is checked here, so an oversized clip fails before it is
    /// uploaded rather than after.
    pub async fn moderate_video(&self, video: &[u8]) -> Result<VideoModerationResponse> {
        self.moderate_video_with(video, None, None).await
    }

    /// [`Self::moderate_video`] with an explicit model and label set.
    pub async fn moderate_video_with(
        &self,
        video: &[u8],
        model: Option<ModerationModel>,
        enabled_labels: Option<Vec<ModerationLabel>>,
    ) -> Result<VideoModerationResponse> {
        if video.len() as i64 > MAX_VIDEO_BYTES {
            return Err(SupervisorError::Validation(format!(
                "video is {} bytes, limit is {} ({}MB)",
                video.len(),
                MAX_VIDEO_BYTES,
                MAX_VIDEO_BYTES / (1024 * 1024)
            )));
        }
        let request = VideoModerationRequest {
            video: base64::engine::general_purpose::STANDARD.encode(video),
            model,
            enabled_labels,
            include_implicit: false,
        };
        self.request(
            reqwest::Method::POST,
            "/api/moderation/user/video",
            Some(&request),
        )
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

    /// Get all available moderation labels as a map of label name to description.
    pub async fn get_labels(&self) -> Result<HashMap<String, String>> {
        self.request::<HashMap<String, String>>(
            reqwest::Method::GET,
            "/api/labels",
            None::<&()>.as_ref(),
        )
        .await
    }
}
