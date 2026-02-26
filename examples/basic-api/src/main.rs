use rust_api::prelude::*;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod controllers;
mod models;
mod services;

use controllers::{
    admin_controller::AdminController,
    echo_controller::EchoController,
    health_controller::HealthController,
    metrics_controller::MetricsController,
    user_controller::UserController,
};
use services::{
    admin_service::AdminService,
    echo_service::EchoService,
    health_service::HealthService,
    metrics_service::MetricsService,
    user_service::UserService,
};

/// GET / — stateless root endpoint, mounted directly on the pipeline.
#[get("/")]
async fn root() -> &'static str {
    "Welcome to RustAPI!"
}

/// Main entry point.
///
/// Demonstrates the full `RouterPipeline` feature set:
///
/// - `group`           — nest controllers under a versioned path prefix
/// - `mount`           — unconditional Kleisli controller bind
/// - `mount_if`        — optional bind; silently skipped when condition is false
/// - `mount_guarded`   — startup refusal when required config is absent
/// - `require_bearer`  — Tower auth transform scoped to a route group
/// - `layer_all`       — fold a `Vec<RouterTransform>` over `map`
/// - `route`           — stateless route (no controller, no service state)
///
/// # Protected Route Pattern
///
/// Admin routes use the correct protected-route model:
///
/// ```
/// .group("/admin", |g| g
///     .mount_guarded(svc, || key_present_check)  // startup refusal
///     .map(require_bearer(key))                   // auth applied to the group
/// )
/// ```
///
/// - `mount_guarded` is a **build-time** check: if `ADMIN_API_KEY` is unset
///   the pipeline short-circuits and the server refuses to start.
///
/// - `require_bearer` is a **request-time** Tower layer. It is applied via
///   `.map()` inside the `/admin` group, so only routes in that group require
///   authentication. `AdminController` has zero auth knowledge.
///
/// This mirrors NestJS Guards / Next.js middleware: auth is a cross-cutting
/// concern applied at the routing layer, not inside handler logic.
#[tokio::main]
async fn main() {
    initialize_tracing();

    // Services are plain Arc<S> — no registry, no type-map.
    let health_svc = Arc::new(HealthService::new());
    let echo_svc = Arc::new(EchoService::new());
    let metrics_svc = Arc::new(MetricsService::new());
    let admin_svc = Arc::new(AdminService::new());
    let user_svc = Arc::new(UserService::new());

    // Read the bearer token once at startup.
    // unwrap_or_default so construction never panics — mount_guarded is the gate.
    let admin_key = std::env::var("ADMIN_API_KEY").unwrap_or_default();

    let app = RouterPipeline::new()
        // ── Public API ──────────────────────────────────────────────────────
        // group: all public routes live under /api/v1 (scoped functor).
        .group("/api/v1", |g| g
            .mount::<HealthController>(health_svc)
            .mount::<EchoController>(echo_svc)
            .mount::<UserController>(user_svc)
        )
        // mount_if: metrics wired only when ENABLE_METRICS is set.
        // When false the pipeline passes through unchanged — no error, no routes.
        .mount_if::<MetricsController>(
            std::env::var("ENABLE_METRICS").is_ok(),
            metrics_svc,
        )
        // ── Protected Admin Group ────────────────────────────────────────────
        // group("/admin", ...) scopes all admin routes under /admin/*.
        //
        //   Step 1 — mount_guarded: startup refusal.
        //     If ADMIN_API_KEY is absent the pipeline returns Err here and
        //     .build() propagates it — the server refuses to start.
        //     This is intentional: enabling admin routes without a key is
        //     a misconfiguration, not a feature toggle.
        //
        //   Step 2 — .map(require_bearer(key)): request-time auth.
        //     Applied inside the group so it covers exactly the /admin/*
        //     routes. Every request is rejected with 401 before any handler
        //     body executes. AdminController has no knowledge of this layer.
        .group("/admin", |g| {
            let key = admin_key.clone();
            g.mount_guarded::<AdminController, _>(admin_svc, move || {
                if key.is_empty() {
                    Err(Error::other(
                        "ADMIN_API_KEY must be set — server will not start without it",
                    ))
                } else {
                    Ok(())
                }
            })
            .map(require_bearer(admin_key.clone()))
        })
        // ── Stateless root ──────────────────────────────────────────────────
        .route(__root_route, root)
        // ── Global middleware ───────────────────────────────────────────────
        // layer_all: fold a runtime Vec<RouterTransform> over map.
        // Equivalent to chaining .map() but accepts a dynamically-built list.
        .layer_all(vec![
            Box::new(|r: Router<()>| r.layer(TraceLayer::new_for_http())) as RouterTransform,
            Box::new(|r: Router<()>| r.layer(CorsLayer::permissive()))    as RouterTransform,
        ])
        .build()
        .expect("Failed to build router");

    RustAPI::new(app)
        .port(3000)
        .serve()
        .await
        .expect("Failed to start server");
}

/// Initializes structured tracing/logging.
fn initialize_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rust_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
