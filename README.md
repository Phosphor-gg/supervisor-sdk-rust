# Supervisor Rust SDK

Official Rust SDK for the [Supervisor](https://supervisor.gg) content moderation API.

Re-exports types directly from `supervisor-types`, zero type duplication.

## Installation

```toml
[dependencies]
supervisor-sdk = { git = "https://github.com/Phosphor-gg/supervisor-sdk-rust" }
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use supervisor_sdk::{SupervisorClient, ModerationRequest, ModerationModel};

#[tokio::main]
async fn main() -> supervisor_sdk::Result<()> {
    let client = SupervisorClient::new("sk-...")?;

    let result = client.moderate(ModerationRequest {
        text: Some("check this text".into()),
        image: None,
        model: Some(ModerationModel::Sentinel),
        enabled_labels: None,
        include_context: false,
        include_implicit: false,
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
    image: None,
    model: Some(ModerationModel::Arbiter),
    enabled_labels: Some(vec![ModerationLabel::H, ModerationLabel::T]),
    include_context: false,
    include_implicit: false,
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
use supervisor_sdk::{
    PlatformClient, PlatformModerationRequest, PlatformCheckoutRequest,
    PlatformChangePlanRequest, Tier, BillingCycle,
};

let platform = PlatformClient::new("client-id", "client-secret")?;

// Provision a user
let user = platform.provision_user("user@example.com").await?;

// Moderate on behalf of a user
let result = platform.moderate(PlatformModerationRequest {
    user_email: "user@example.com".into(),
    text: Some("check this".into()),
    image: None,
    model: None,
    enabled_labels: None,
    include_context: false,
    include_implicit: false,
}).await?;

// Create checkout
let checkout = platform.create_checkout(PlatformCheckoutRequest {
    user_email: "user@example.com".into(),
    tier: Tier::Standard,
    billing_cycle: BillingCycle::Monthly,
    success_url: "https://yourapp.com/success".into(),
    cancel_url: "https://yourapp.com/cancel".into(),
}).await?;

// Change the plan of an existing subscription
let change = platform.change_plan(PlatformChangePlanRequest {
    user_email: "user@example.com".into(),
    tier: Tier::Premium,
    billing_cycle: BillingCycle::Annual,
}).await?;
println!("Now on {:?} ({:?})", change.tier, change.billing_cycle);

// List linked users
let users = platform.list_users().await?;

// Get a specific linked user by ID
let user = platform.get_user("user-id").await?;
println!("Authorized: {}, Tier: {:?}", user.authorized, user.tier);

// Confirm a user's authorization with the code from the redirect
let confirmed = platform.confirm_authorization("auth-code").await?;
println!("Authorized {} ({})", confirmed.email, confirmed.user_id);

// Check Stripe Connect onboarding status
let status = platform.get_connect_status().await?;
println!("Onboarding complete: {}", status.onboarding_complete);
```

### Checkout and plan changes

`create_checkout` returns a 403 error if the user has not authorized the platform, and a 400 error if the user already has an active subscription (use `change_plan` instead). `change_plan` returns a 403 error if the subscription was not originated by this platform, and a 400 error if the user has no active subscription. Revenue share is set at subscription creation and preserved across plan changes.

### Products and checkout links

Platforms sell Supervisor plans and credit packs from their own site. List the products, render them however you like, and when a user clicks, mint a per-user checkout link and redirect. Revenue share applies to both product types.

```rust
let products = platform.get_products().await?;
// products.plans: subscription tiers with prices in cents
// products.credit_packs: one-time credit packs

// Credit pack checkout (one-time payment)
let credits = platform
    .create_credit_checkout(PlatformCreditCheckoutRequest {
        user_email: "user@example.com".to_string(),
        price_id: products.credit_packs[0].price_id.clone(),
        success_url: "https://myapp.com/thanks".to_string(),
        cancel_url: "https://myapp.com/pricing".to_string(),
    })
    .await?;
// redirect the user to credits.checkout_url
```

Show an authorized user their remaining credits:

```rust
let balance = platform.get_user_credits(&user_id).await?;
// balance.balance is the total usable right now; monthly and extra breakdowns included
```

Returns 403 if the user has not authorized your platform.

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
