# RustAPI

> **FastAPI-inspired REST framework for Rust — compositional, type-safe, compile-time guarantees**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)

Spin up a REST API in Rust with the developer experience of FastAPI and NestJS —
declarative routes, accumulating validation, scoped auth middleware — built on
Axum and Tokio with compile-time guarantees throughout.

**Status**: Active development. Not yet production-ready.

---

## Quick Start

```rust
use rust_api::prelude::*;

#[get("/health")]
pub async fn health_check(State(svc): State<Arc<HealthService>>) -> Json<HealthResponse> {
    Json(svc.health_check())
}

pub struct HealthController;
mount_handlers!(HealthController, HealthService, [(__health_check_route, health_check)]);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = RouterPipeline::new()
        .mount::<HealthController>(Arc::new(HealthService::new()))
        .build()?;

    RustAPI::new().serve(app).await
}
```

---

## Features

| Feature | Description |
|---|---|
| **Kleisli Pipeline** | `RouterPipeline` composes controllers as Kleisli arrows (`>=>`). Each `.mount` applies the arrow via `>>=` internally; errors short-circuit at `build()` — never at runtime. |
| **Route Macros** | `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` — the HTTP verb is a binding contract enforced at registration. |
| **Pure Controllers** | Controllers are zero-knowledge marker types. No `axum` imports, no auth logic, no routing infrastructure in handler code. |
| **Scoped Auth** | `require_bearer(key)` and `guard(header, key)` are Tower layers applied to route groups via `.map()`. |
| **Conditional Mounting** | `.mount_if(condition, svc)` silently skips. `.mount_guarded(svc, guard)` refuses to start if the guard fails. |
| **Applicative Validation** | `#[derive(Validatable)]` accumulates all field errors. `ValidatedJson<T>` / `ValidatedQuery<T>` return `422` with every error before the handler runs. |
| **Smart Constructors** | `#[derive(NewType)]` generates validated value-object newtypes with `Deref`, `Display`, and `Serialize`/`Deserialize`. |
| **Phantom-typed IDs** | `Id<T>` wraps `Uuid` branded with the entity type. `Id<User>` and `Id<Post>` are distinct compile-time types. |
| **Repository abstraction** | `Repository<T, Id>` trait with composable `QuerySpec` (filters, ordering, paging). `InMemoryRepository<T, Id>` is the bundled dev-time implementation — swap in a real adapter (sqlx, etc.) without touching services or controllers. |
| **Prelude** | `use rust_api::prelude::*` — one import. `axum` is never a direct user dependency. |

### Roadmap

- **OpenAPI generation** — the pipeline retains path/verb/type metadata at every layer. `RouterPipeline::build_with_openapi()` is the natural next step.
- **sqlx / postgres adapter** — `Repository<T, Id>` trait is the interface; `InMemoryRepository` is the prototype.
- **TypeScript SDK generation** — full-stack type safety from Rust types to TS client.

---

## Building and Running

### Prerequisites

- Rust 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Optional: [`just`](https://just.systems) task runner (`cargo install just`)

### Build

```bash
git clone https://github.com/Andyrug/rust-api
cd rust-api
cargo build
```

### Run the example API

```bash
# Health, echo, and user routes
cargo run -p basic-api

# With admin routes enabled (protected by bearer token)
ADMIN_API_KEY=secret cargo run -p basic-api

# With metrics endpoint
ENABLE_METRICS=1 cargo run -p basic-api

# Everything on
ADMIN_API_KEY=secret ENABLE_METRICS=1 cargo run -p basic-api
```

Using `just`:

```bash
just run        # minimal
just run-admin  # admin routes
just run-full   # all features
```

### Test the endpoints

```bash
# Health
curl http://localhost:3000/api/v1/health

# Echo
curl -X POST http://localhost:3000/api/v1/echo \
     -H "Content-Type: application/json" \
     -d '{"message": "hello"}'

# Create user
curl -X POST http://localhost:3000/api/v1/users \
     -H "Content-Type: application/json" \
     -d '{"username": "alice_99", "email": "alice@example.com"}'

# Admin (requires ADMIN_API_KEY=secret at startup)
curl http://localhost:3000/admin/status \
     -H "Authorization: Bearer secret"
```

---

## Tests

```bash
cargo test --workspace   # or: just test
```



For a coverage report (requires [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)):

```bash
cargo install cargo-llvm-cov
just cov          # opens HTML report
just cov-lcov     # LCOV for CI
```

---

## Docker

A multi-stage Dockerfile is included. The final image contains only the binary.

```bash
# Build
docker build -t rust-api .

# Run
docker run -p 3000:3000 rust-api

# With env vars
docker run -p 3000:3000 \
  -e ADMIN_API_KEY=secret \
  -e ENABLE_METRICS=1 \
  -e RUST_LOG=debug \
  rust-api
```

Port `3000` is exposed. `RUST_LOG` defaults to `info`.

---

## Architecture

```
rust-api/
├── crates/
│   ├── rust-api/               # Framework
│   │   └── src/
│   │       ├── pipeline.rs     # RouterPipeline — Kleisli composition
│   │       ├── controller.rs   # Controller trait
│   │       ├── middleware.rs   # require_bearer, guard
│   │       ├── validation.rs   # Validatable, ValidatedJson, HandlerResult
│   │       ├── validators.rs   # email, url, uuid, zip, phone, …
│   │       ├── repository.rs   # Repository<T,Id>, InMemoryRepository<T,Id>
│   │       └── id.rs           # Id<T> — phantom-typed UUID
│   └── rust-api-macros/        # #[get/#[post]/…, #[derive(Validatable)], #[derive(NewType)]
├── examples/
│   └── basic-api/              # End-to-end example
│       └── src/
│           ├── main.rs         # RouterPipeline composition
│           ├── controllers/    # Pure handlers + mount_handlers!
│           ├── services/       # Business logic, no HTTP types
│           └── models/
│               ├── domain.rs   # Aggregates, value objects, Id<User>
│               ├── requests.rs # DTOs with #[derive(Validatable)]
│               └── responses.rs
├── docs/
│   ├── ARCHITECTURE.md
│   └── CompositionalRefactor.md
├── Dockerfile
└── justfile
```

### The Pipeline

`RouterPipeline` is a monadic chain over `Result<Router>`. Each step is a
**`Router<()> → Result<Router<()>>`** Kleisli arrow threaded via `Result::and_then`.

```rust
RouterPipeline::new()
    .group("/api/v1", |g| g
        .mount::<HealthController>(health_svc)   // Kleisli bind
        .mount::<EchoController>(echo_svc)
    )
    .mount_if::<MetricsController>(config.metrics, metrics_svc)
    .group("/admin", |g| g
        .mount_guarded::<AdminController, _>(admin_svc, || guard_check())
        .map(require_bearer(admin_key))           // auth scoped to /admin only
    )
    .build()?
```

| Method | FP concept | Effect |
|---|---|---|
| `.mount::<C>(svc)` | Kleisli composition `>=>` (uses `>>=` internally) | Compose controller's arrow into the pipeline |
| `.map(f)` | Functor `fmap` | Infallible `Router → Router` transform |
| `.mount_if(cond, svc)` | Conditional `>=>` | Compose only when condition is true |
| `.mount_guarded(svc, g)` | Guarded `>=>` | Compose or short-circuit at startup |
| `.group(prefix, \|g\| …)` | Scoped functor | Sub-pipeline with path prefix |
| `.layer_all(transforms)` | Catamorphism | Apply a list of `Router → Router` transforms |
| `.build()` | Interpreter | Unwrap into `Result<Router>` |

### Validation

One validation story, applied at every layer:

```
ValidatedJson<Req>    — accumulates field errors at HTTP edge → 422
Username::new()       — single-field invariant, smart constructor
Email::new()          — single-field invariant, smart constructor
User::validate()      — cross-field aggregate invariant
repo.save(user)       — only reached when all invariants satisfied
```

Handler signatures use `HandlerResult<Json<T>>` — a concrete type alias for
`std::result::Result<Json<T>, ValidationRejection>`, not `impl IntoResponse`.
Both branches are compiler-verified and inspectable by future OpenAPI tooling.

### Identity

`Id<T>` is a `Uuid` branded with the entity type at compile time:

```rust
pub struct Id<T> {
    value: Uuid,
    _marker: PhantomData<fn() -> T>,  // zero bytes at runtime
}
```

This is currently in active development. Contributions welcome!

---

This project is licensed under either of:

- Apache License, Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0)
- MIT license (http://opensource.org/licenses/MIT)

at your option.

## Inspiration

- **FastAPI** (Python) — declarative routes, automatic validation, clean DX
- **NestJS** (TypeScript) — compositional modules, Guards, Middleware as layers
- **Axum** (Rust) — ergonomic, type-safe, production-grade HTTP
- **Giraffe** (F#) — Kleisli HTTP handlers, the fish operator `>=>`

---

Built using Rust, Axum, and Tokio.
