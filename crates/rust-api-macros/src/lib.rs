//! Procedural macros for rust-api framework
//!
//! Provides route macros like #[get], #[post], etc. for defining HTTP endpoints
//! in a FastAPI-style syntax, and `#[derive(Validatable)]` for applicative
//! field validation.

use proc_macro::TokenStream;

mod route;
mod rules;
mod validatable;
mod newtype;

use route::HttpMethod;

/// Define a GET route handler
///
/// # Example
///
/// ```ignore
/// #[get("/users/:id")]
/// async fn get_user(path: Path<String>) -> Json<User> {
///     // handler code
/// }
/// ```
#[proc_macro_attribute]
pub fn get(args: TokenStream, input: TokenStream) -> TokenStream {
    route::expand_route_macro(HttpMethod::Get, args, input)
}

/// Define a POST route handler
///
/// # Example
///
/// ```ignore
/// #[post("/users")]
/// async fn create_user(body: Json<CreateUser>) -> Json<User> {
///     // handler code
/// }
/// ```
#[proc_macro_attribute]
pub fn post(args: TokenStream, input: TokenStream) -> TokenStream {
    route::expand_route_macro(HttpMethod::Post, args, input)
}

/// Define a PUT route handler
///
/// # Example
///
/// ```ignore
/// #[put("/users/:id")]
/// async fn update_user(path: Path<String>, body: Json<User>) -> Json<User> {
///     // handler code
/// }
/// ```
#[proc_macro_attribute]
pub fn put(args: TokenStream, input: TokenStream) -> TokenStream {
    route::expand_route_macro(HttpMethod::Put, args, input)
}

/// Define a DELETE route handler
///
/// # Example
///
/// ```ignore
/// #[delete("/users/:id")]
/// async fn delete_user(path: Path<String>) -> StatusCode {
///     // handler code
/// }
/// ```
#[proc_macro_attribute]
pub fn delete(args: TokenStream, input: TokenStream) -> TokenStream {
    route::expand_route_macro(HttpMethod::Delete, args, input)
}

/// Define a PATCH route handler
///
/// # Example
///
/// ```ignore
/// #[patch("/users/:id")]
/// async fn patch_user(path: Path<String>, body: Json<UserPatch>) -> Json<User> {
///     // handler code
/// }
/// ```
#[proc_macro_attribute]
pub fn patch(args: TokenStream, input: TokenStream) -> TokenStream {
    route::expand_route_macro(HttpMethod::Patch, args, input)
}

/// Derive the `Validatable` trait for a struct using field-level `#[validate(...)]` attributes.
///
/// Generates a `Validatable::validate(self)` implementation as a pure
/// `frunk::Validated` accumulation chain — all errors are collected, nothing
/// short-circuits.
///
/// # Supported attributes
///
/// ```ignore
/// #[derive(Deserialize, Validatable)]
/// pub struct CreateUserRequest {
///     #[validate(email)]
///     pub email: String,
///     #[validate(min_length = 3, max_length = 50)]
///     pub username: String,
///     #[validate(us_zip)]
///     pub zip: String,
///     #[validate(range(min = 18, max = 120))]
///     pub age: u32,
///     #[validate(custom(predicate = "is_not_reserved", message = "reserved word"))]
///     pub handle: String,
/// }
/// ```
#[proc_macro_derive(Validatable, attributes(validate))]
pub fn derive_validatable(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    validatable::expand_validatable(input).into()
}

/// Derive a smart-constructor newtype wrapper for a single-field tuple struct.
///
/// Generates: `new()`, `into_inner()`, `Deref`, `Display`, `Serialize`,
/// `Deserialize` (validates on deserialisation), and `Validatable`.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Clone, NewType)]
/// pub struct Email(#[validate(email)] String);
///
/// #[derive(Debug, Clone, NewType)]
/// pub struct Username(
///     #[validate(min_length = 3)]
///     #[validate(max_length = 30)]
///     String
/// );
///
/// // Validated construction:
/// let email = Email::new("user@example.com")?; // Ok
/// let email = Email::new("not-an-email");       // Err(vec![FieldError { .. }])
/// ```
#[proc_macro_derive(NewType, attributes(validate))]
pub fn derive_newtype(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    newtype::expand_newtype(input).into()
}
