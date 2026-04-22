# Supervisor Rust SDK

Official Rust SDK for the [Supervisor](https://supervisor.gg) content moderation API.

Re-exports types directly from `supervisor-types` — zero type duplication.

## Installation

```toml
[dependencies]
supervisor-sdk = { git = "https://github.com/phosphor-tech/supervisor-sdk-rust" }
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use supervisor_sdk::{SupervisorClient, ModerationRequest, ModerationModel};

#[tokio::main]
async fn main() -> supervisor_sdk::Result<()> {
    let client = SupervisorClient::new("sk-...");

    let result = client.moderate(ModerationRequest {
        text: Some("check this text".into()),
        model: Some(ModerationModel::Sentinel),
        ..Default::default()
    }).await?;

    println!("Flagged: {}", result.flagged);
    println!("Labels: {:?}", result.labels);
    Ok(())
}
```

## Usage

### Moderate Text

```rust
let result = client.moderate(ModerationRequest {
    text: Some("some text to check".into()),
    model: Some(ModerationModel::Arbiter),
    enabled_labels: Some(vec![ModerationLabel::H, ModerationLabel::T]),
    ..Default::default()
}).await?;
```

### Batch Moderation

```rust
use supervisor_sdk::BatchModerationRequest;

let results = client.moderate_batch(BatchModerationRequest {
    texts: vec!["first".into(), "second".into(), "third".into()],
    ..Default::default()
}).await?;

for result in results {
    println!("Flagged: {}, Labels: {:?}", result.flagged, result.labels);
}
```

### Username Check

```rust
let result = client.check_username("username123").await?;
println!("Flagged: {}, Score: {}", result.flagged, result.score);
```

### Get Labels

```rust
let labels = client.get_labels().await?;
```

## Platform API

```rust
use supervisor_sdk::{PlatformClient, PlatformModerationRequest, PlatformCheckoutRequest, Tier, BillingCycle};

let platform = PlatformClient::new("client-id", "client-secret");

// Provision a user
let user = platform.provision_user("user@example.com").await?;

// Moderate on behalf of a user
let result = platform.moderate(PlatformModerationRequest {
    user_email: "user@example.com".into(),
    text: Some("check this".into()),
    ..Default::default()
}).await?;

// Create checkout
let checkout = platform.create_checkout(PlatformCheckoutRequest {
    user_email: "user@example.com".into(),
    tier: Tier::Standard,
    billing_cycle: BillingCycle::Monthly,
    success_url: "https://yourapp.com/success".into(),
    cancel_url: "https://yourapp.com/cancel".into(),
}).await?;

// List linked users
let users = platform.list_users().await?;
```

## Error Handling

```rust
use supervisor_sdk::SupervisorError;

match client.moderate(request).await {
    Ok(result) => println!("Flagged: {}", result.flagged),
    Err(SupervisorError::Api { status_code: 401, .. }) => {
        eprintln!("Invalid API key");
    }
    Err(SupervisorError::Api { status_code: 429, .. }) => {
        eprintln!("Rate limited");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## License

MIT
