//! # Supervisor SDK
//!
//! Official Rust SDK for the [Supervisor](https://supervisor.gg) content moderation API.
//!
//! ## Quick Start
//!
//! ```no_run
//! use supervisor_sdk::{SupervisorClient, ModerationRequest, ModerationModel};
//!
//! #[tokio::main]
//! async fn main() -> supervisor_sdk::Result<()> {
//!     let client = SupervisorClient::new("sk-...");
//!
//!     let result = client.moderate(ModerationRequest {
//!         text: Some("check this text".into()),
//!         model: Some(ModerationModel::Sentinel),
//!         ..Default::default()
//!     }).await?;
//!
//!     println!("Flagged: {}", result.flagged);
//!     println!("Labels: {:?}", result.labels);
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod partner;

// Re-export the client types
pub use client::SupervisorClient;
pub use error::{Result, SupervisorError};
pub use partner::PartnerClient;

// Re-export types from supervisor-types
pub use supervisor_types::moderate::{
    BatchModerationRequest, ModerationLabel, ModerationModel, ModerationRequest,
    ModerationResponse, UsernameCheckRequest, UsernameCheckResponse,
};
pub use supervisor_types::partner::{
    ConfirmAuthorizationRequest, ConfirmAuthorizationResponse, PartnerCheckoutRequest,
    PartnerCheckoutResponse, PartnerModerationRequest, PartnerTokenRequest,
    PartnerTokenResponse, PartnerUserInfo, ProvisionUserRequest, ProvisionUserResponse,
    StripeConnectStatusResponse,
};
pub use supervisor_types::pricing::{BillingCycle, Tier};
