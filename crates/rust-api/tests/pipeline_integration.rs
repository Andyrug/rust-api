//! Integration tests for `RouterPipeline`.
//!
//! Each test builds a minimal router via the public API, fires an in-process
//! request using `tower::ServiceExt::oneshot`, and asserts on status + body.
//! No TCP server is started — requests are processed entirely in-process.

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use rust_api::prelude::*;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Test services
// ---------------------------------------------------------------------------

pub struct PingService;

impl PingService {
    pub fn new() -> Self {
        Self
    }
    pub fn ping(&self) -> &'static str {
        "pong"
    }
}

pub struct MessageService;

impl MessageService {
    pub fn new() -> Self {
        Self
    }
    pub fn greet(&self, name: &str) -> String {
        format!("hello, {name}")
    }
}

// ---------------------------------------------------------------------------
// Test controllers
// ---------------------------------------------------------------------------

#[get("/ping")]
pub async fn ping_handler(State(svc): State<Arc<PingService>>) -> &'static str {
    svc.ping()
}

pub struct PingController;
mount_handlers!(
    PingController,
    PingService,
    [(__ping_handler_route, ping_handler)]
);

#[derive(Serialize, Deserialize)]
pub struct GreetRequest {
    pub name: String,
}

#[post("/greet")]
pub async fn greet_handler(
    State(svc): State<Arc<MessageService>>,
    Json(body): Json<GreetRequest>,
) -> String {
    svc.greet(&body.name)
}

pub struct MessageController;
mount_handlers!(
    MessageController,
    MessageService,
    [(__greet_handler_route, greet_handler)]
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn body_string(body: axum::body::Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn get_request(app: Router<()>, uri: &str) -> (u16, String) {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let body = body_string(resp.into_body()).await;
    (status, body)
}

// ---------------------------------------------------------------------------
// Tests — basic routing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_ping_returns_200_with_body() {
    let app = RouterPipeline::new()
        .mount::<PingController>(Arc::new(PingService::new()))
        .build()
        .unwrap();

    let (status, body) = get_request(app, "/ping").await;
    assert_eq!(status, 200);
    assert_eq!(body, "pong");
}

#[tokio::test]
async fn post_greet_returns_200_with_greeting() {
    let app = RouterPipeline::new()
        .mount::<MessageController>(Arc::new(MessageService::new()))
        .build()
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/greet")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Alice"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    let body = body_string(resp.into_body()).await;
    assert_eq!(body, "hello, Alice");
}

// ---------------------------------------------------------------------------
// Tests — verb enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_on_post_only_route_returns_405() {
    let app = RouterPipeline::new()
        .mount::<MessageController>(Arc::new(MessageService::new()))
        .build()
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/greet")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status().as_u16(),
        405,
        "GET on a POST-only route should return 405"
    );
}

// ---------------------------------------------------------------------------
// Tests — require_bearer auth middleware
// ---------------------------------------------------------------------------

fn authed_app(token: &str) -> Router<()> {
    RouterPipeline::new()
        .mount::<PingController>(Arc::new(PingService::new()))
        .map(require_bearer(token.to_owned()))
        .build()
        .unwrap()
}

#[tokio::test]
async fn correct_bearer_token_returns_200() {
    let app = authed_app("my-secret");
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ping")
                .header("Authorization", "Bearer my-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

#[tokio::test]
async fn wrong_bearer_token_returns_401() {
    let app = authed_app("my-secret");
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ping")
                .header("Authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn missing_auth_header_returns_401() {
    let app = authed_app("my-secret");
    let (status, _) = get_request(app, "/ping").await;
    assert_eq!(status, 401);
}

// ---------------------------------------------------------------------------
// Tests — mount_if
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mount_if_false_route_returns_404() {
    let app = RouterPipeline::new()
        .mount_if::<PingController>(false, Arc::new(PingService::new()))
        .build()
        .unwrap();

    let (status, _) = get_request(app, "/ping").await;
    assert_eq!(
        status, 404,
        "route should not be registered when mount_if condition is false"
    );
}

#[tokio::test]
async fn mount_if_true_route_returns_200() {
    let app = RouterPipeline::new()
        .mount_if::<PingController>(true, Arc::new(PingService::new()))
        .build()
        .unwrap();

    let (status, _) = get_request(app, "/ping").await;
    assert_eq!(status, 200);
}

// ---------------------------------------------------------------------------
// Tests — mount_guarded
// ---------------------------------------------------------------------------

#[test]
fn mount_guarded_failing_guard_causes_build_to_err() {
    let result = RouterPipeline::new()
        .mount_guarded::<PingController, _>(Arc::new(PingService::new()), || {
            Err(Error::other("required config missing"))
        })
        .build();

    assert!(result.is_err(), "build() must return Err when guard fails");
}

#[tokio::test]
async fn mount_guarded_passing_guard_registers_route() {
    let app = RouterPipeline::new()
        .mount_guarded::<PingController, _>(Arc::new(PingService::new()), || Ok(()))
        .build()
        .unwrap();

    let (status, _) = get_request(app, "/ping").await;
    assert_eq!(status, 200);
}

// ---------------------------------------------------------------------------
// Tests — group prefix
// ---------------------------------------------------------------------------

#[tokio::test]
async fn group_prefix_applied_to_nested_routes() {
    let app = RouterPipeline::new()
        .group("/api/v1", |g| {
            g.mount::<PingController>(Arc::new(PingService::new()))
        })
        .build()
        .unwrap();

    let (prefixed_status, _) = get_request(app.clone(), "/api/v1/ping").await;
    let (bare_status, _) = get_request(app, "/ping").await;

    assert_eq!(prefixed_status, 200, "/api/v1/ping should be 200");
    assert_eq!(bare_status, 404, "/ping without prefix should be 404");
}

#[tokio::test]
async fn group_auth_scoped_to_group_only() {
    let app = RouterPipeline::new()
        .mount::<PingController>(Arc::new(PingService::new()))
        .group("/admin", |g| {
            g.mount::<MessageController>(Arc::new(MessageService::new()))
                .map(require_bearer("admin-token"))
        })
        .build()
        .unwrap();

    // Public route accessible without auth
    let (pub_status, _) = get_request(app.clone(), "/ping").await;
    assert_eq!(pub_status, 200, "public route should not require auth");

    // Admin route blocked without token
    let admin_no_token = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/greet")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"Bob"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        admin_no_token.status().as_u16(),
        401,
        "admin route should require auth"
    );

    // Admin route accessible with correct token
    let admin_with_token = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/greet")
                .header("content-type", "application/json")
                .header("Authorization", "Bearer admin-token")
                .body(Body::from(r#"{"name":"Bob"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        admin_with_token.status().as_u16(),
        200,
        "admin route should succeed with correct token"
    );
}

// ---------------------------------------------------------------------------
// Tests — applicative validation (ValidatedJson)
// ---------------------------------------------------------------------------

/// Minimal service + controller wired with ValidatedJson for the tests below.
struct ItemService;

impl ItemService {
    fn new() -> Self {
        Self
    }
    fn name(&self, n: &str) -> String {
        format!("created: {n}")
    }
}

#[derive(Serialize, Deserialize, Validatable)]
struct CreateItemRequest {
    #[validate(min_length = 3)]
    #[validate(max_length = 20)]
    pub name: String,
    #[validate(email)]
    pub owner_email: String,
}

#[post("/items")]
async fn create_item(
    State(svc): State<Arc<ItemService>>,
    ValidatedJson(req): ValidatedJson<CreateItemRequest>,
) -> String {
    svc.name(&req.name)
}

struct ItemController;
mount_handlers!(ItemController, ItemService, [(__create_item_route, create_item)]);

fn item_app() -> Router<()> {
    RouterPipeline::new()
        .mount::<ItemController>(Arc::new(ItemService::new()))
        .build()
        .unwrap()
}

#[tokio::test]
async fn valid_payload_returns_200() {
    let resp = item_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/items")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"widget","owner_email":"a@b.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}

#[tokio::test]
async fn invalid_payload_returns_422() {
    let resp = item_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/items")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"ab","owner_email":"notanemail"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        422,
        "invalid fields should return 422"
    );
}

#[tokio::test]
async fn all_errors_accumulated_not_short_circuited() {
    let resp = item_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/items")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"ab","owner_email":"notanemail"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let errors = parsed["errors"].as_array().unwrap();
    assert!(
        errors.len() >= 2,
        "expected at least 2 accumulated errors, got {}: {}",
        errors.len(),
        body
    );
}
