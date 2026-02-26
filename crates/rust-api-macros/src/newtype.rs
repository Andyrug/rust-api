//! Implementation of the `#[derive(NewType)]` macro.
//!
//! Generates a smart-constructor newtype wrapper for a single-field tuple struct.
//!
//! # Usage
//!
//! ```ignore
//! #[derive(Debug, Clone, NewType)]
//! pub struct Email(#[validate(email)] String);
//!
//! #[derive(Debug, Clone, NewType)]
//! pub struct Username(
//!     #[validate(min_length = 3)]
//!     #[validate(max_length = 30)]
//!     #[validate(matches = "^[a-z0-9_]+$")]
//!     String
//! );
//! ```
//!
//! # Generated impls
//!
//! For `Email(String)` with `#[validate(email)]`:
//!
//! ```ignore
//! impl Email {
//!     pub fn new(value: impl Into<String>) -> ValidationResult<Self> { ... }
//!     pub fn into_inner(self) -> String { self.0 }
//!     pub fn as_str(&self) -> &str { &self.0 }  // only when inner == String
//! }
//! impl Deref for Email { type Target = String; ... }
//! impl Display for Email { ... }
//! impl Serialize for Email { ... }        // transparent — serializes inner value
//! impl Deserialize for Email { ... }      // validates on deserialization
//! impl Validatable for Email { ... }      // composes into accumulation chains
//! ```
//!
//! # OpenAPI note
//!
//! `#[validate(...)]` attributes are parsed into the structured `ValidateRule` enum,
//! not collapsed to strings. A future schema-codegen pass can walk the same enum to
//! emit OpenAPI properties (`email` → `"format": "email"`, etc.).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type};

use crate::rules::{emit_call, parse_validate_attrs};

pub fn expand_newtype(input: DeriveInput) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Build a modified generics set that adds the `'__de` lifetime for the
    // Deserialize impl — standard pattern for parameterising over a lifetime.
    let mut de_generics = input.generics.clone();
    de_generics.params.insert(0, syn::parse_quote!('__de));
    let (de_impl_generics, _, _) = de_generics.split_for_impl();

    // Must be a tuple struct with exactly one unnamed field.
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Unnamed(f) => &f.unnamed,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "#[derive(NewType)] requires a tuple struct: `pub struct Foo(InnerType);`",
                )
                .to_compile_error()
            }
        },
        _ => {
            return syn::Error::new_spanned(
                name,
                "#[derive(NewType)] only supports tuple structs",
            )
            .to_compile_error()
        }
    };

    if fields.len() != 1 {
        return syn::Error::new_spanned(
            name,
            "#[derive(NewType)] requires exactly one inner field",
        )
        .to_compile_error();
    }

    let field = &fields[0];
    let inner_type = &field.ty;

    // Parse `#[validate(...)]` attributes on the inner field.
    let rules = match parse_validate_attrs(&field.attrs) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error(),
    };

    // Build validator call expressions for new() and validate() separately,
    // because the value expression differs:
    //   new()       → variable `inner`  / `&inner`
    //   validate()  → field  `self.0`  / `&self.0`
    let new_ref = quote!(&inner);
    let new_direct = quote!(inner);
    let val_ref = quote!(&self.0);
    let val_direct = quote!(self.0);

    let mut new_calls: Vec<TokenStream> = Vec::new();
    let mut val_calls: Vec<TokenStream> = Vec::new();

    for rule in &rules {
        let nc = match emit_call(rule, "value", &new_ref, &new_direct) {
            Ok(ts) => ts,
            Err(e) => return e.to_compile_error(),
        };
        let vc = match emit_call(rule, "value", &val_ref, &val_direct) {
            Ok(ts) => ts,
            Err(e) => return e.to_compile_error(),
        };
        new_calls.push(nc);
        val_calls.push(vc);
    }

    // Validation body inside new(): accumulate all errors, then ? to propagate.
    // `into_result()` returns Result<HList, Vec<FieldError>>; the ? propagates
    // the error with the correct type since the function returns ValidationResult<Self>.
    let new_validation = if new_calls.is_empty() {
        quote! {}
    } else {
        let first = &new_calls[0];
        let rest = &new_calls[1..];
        quote! {
            use ::rust_api::validation::IntoValidated;
            (#first.into_validated() #(+ #rest)*).into_result()?;
        }
    };

    // Validatable impl body.
    let val_body = if val_calls.is_empty() {
        quote! {
            impl #impl_generics ::rust_api::validation::Validatable
                for #name #ty_generics #where_clause
            {
                fn validate(self) -> ::rust_api::validation::ValidationResult<Self> {
                    ::std::result::Result::Ok(self)
                }
            }
        }
    } else {
        let first = &val_calls[0];
        let rest = &val_calls[1..];
        quote! {
            impl #impl_generics ::rust_api::validation::Validatable
                for #name #ty_generics #where_clause
            {
                fn validate(self) -> ::rust_api::validation::ValidationResult<Self> {
                    use ::rust_api::validation::IntoValidated;
                    (#first.into_validated() #(+ #rest)*).into_result().map(|_| self)
                }
            }
        }
    };

    // `as_str()` is only meaningful when the inner type is `String`.
    let as_str_method = if is_string_type(inner_type) {
        quote! {
            /// Borrow the inner string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Construct a validated value.
            ///
            /// Returns `Ok(Self)` when all checks pass, or `Err(Vec<FieldError>)`
            /// with every accumulated failure — nothing short-circuits.
            pub fn new(
                value: impl Into<#inner_type>,
            ) -> ::rust_api::validation::ValidationResult<Self> {
                let inner: #inner_type = value.into();
                #new_validation
                ::std::result::Result::Ok(Self(inner))
            }

            /// Consume `self` and return the inner value.
            pub fn into_inner(self) -> #inner_type {
                self.0
            }

            #as_str_method
        }

        impl #impl_generics ::std::ops::Deref for #name #ty_generics #where_clause {
            type Target = #inner_type;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl #impl_generics ::std::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }

        impl #impl_generics ::serde::Serialize for #name #ty_generics #where_clause {
            fn serialize<__S: ::serde::Serializer>(
                &self,
                s: __S,
            ) -> ::std::result::Result<__S::Ok, __S::Error> {
                ::serde::Serialize::serialize(&self.0, s)
            }
        }

        // Deserialize by first deserialising the inner type, then validating.
        // This enforces the "parse, don't validate" principle at the serde boundary.
        impl #de_impl_generics ::serde::Deserialize<'__de>
            for #name #ty_generics #where_clause
        {
            fn deserialize<__D: ::serde::Deserializer<'__de>>(
                d: __D,
            ) -> ::std::result::Result<Self, __D::Error> {
                let inner = <#inner_type as ::serde::Deserialize>::deserialize(d)?;
                Self::new(inner).map_err(|errs| {
                    let msg = errs
                        .iter()
                        .map(|e| e.message.as_str())
                        .collect::<::std::vec::Vec<_>>()
                        .join("; ");
                    <__D::Error as ::serde::de::Error>::custom(msg)
                })
            }
        }

        #val_body
    }
}

// ---------------------------------------------------------------------------
// Helper: check if a syn::Type is the bare `String` path
// ---------------------------------------------------------------------------

fn is_string_type(ty: &Type) -> bool {
    if let Type::Path(p) = ty {
        p.path
            .segments
            .last()
            .map(|s| s.ident == "String")
            .unwrap_or(false)
    } else {
        false
    }
}
