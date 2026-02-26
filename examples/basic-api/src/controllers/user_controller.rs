//! User controller — demonstrates `ValidatedJson` at the HTTP boundary and
//! monadic `Result` composition through the service layer.
//!
//! `ValidatedJson<CreateUserRequest>` runs `Validatable::validate()` before
//! the handler body executes — accumulating all field errors and returning
//! `422 Unprocessable Entity` if any fail.
//!
//! The handler returns `HandlerResult<Json<UserResponse>>`:
//! - `Ok(Json(...))` → 200 with JSON body
//! - `Err(ValidationRejection(...))` → 422 with field errors
//!
//! Explicit concrete return type keeps the signature readable, verifiable by
//! the compiler, and inspectable by future OpenAPI generation tooling.

use std::sync::Arc;

use rust_api::prelude::*;

use crate::models::requests::CreateUserRequest;
use crate::models::responses::UserResponse;
use crate::services::user_service::UserService;

#[post("/users")]
pub async fn create_user(
    State(svc): State<Arc<UserService>>,
    ValidatedJson(req): ValidatedJson<CreateUserRequest>,
) -> HandlerResult<Json<UserResponse>> {
    // svc.create() returns ValidationResult<UserResponse>.
    // .map(Json)                    — functor over the Ok branch
    // .map_err(ValidationRejection) — functor over the Err branch, lifts
    //                                 Vec<FieldError> into the rejection type
    svc.create(req).await.map(Json).map_err(ValidationRejection)
}

pub struct UserController;

mount_handlers!(UserController, UserService, [(__create_user_route, create_user)]);
