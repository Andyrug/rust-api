//! Implementation of the `#[derive(Validatable)]` macro.
//!
//! Reads `#[validate(...)]` attributes on struct fields and generates a
//! `Validatable::validate(self)` implementation as a pure `frunk::Validated`
//! accumulation chain.
//!
//! # Supported field attributes
//!
//! | Attribute                                          | Generates call to                         |
//! |----------------------------------------------------|-------------------------------------------|
//! | `#[validate(email)]`                               | `validate_email(field, &self.field)`      |
//! | `#[validate(url)]`                                 | `validate_url(field, &self.field)`        |
//! | `#[validate(uuid)]`                                | `validate_uuid(field, &self.field)`       |
//! | `#[validate(non_empty)]`                           | `validate_non_empty(field, &self.field)`  |
//! | `#[validate(us_zip)]`                              | `validate_us_zip(field, &self.field)`     |
//! | `#[validate(ca_postal)]`                           | `validate_ca_postal(field, &self.field)`  |
//! | `#[validate(phone_e164)]`                          | `validate_phone_e164(field, &self.field)` |
//! | `#[validate(min_length = N)]`                      | `validate_min_length(field, &self.field, N)` |
//! | `#[validate(max_length = N)]`                      | `validate_max_length(field, &self.field, N)` |
//! | `#[validate(range(min = N, max = N))]`             | `validate_range(field, self.field, N, N)` |
//! | `#[validate(matches = "pattern")]`                 | `validate_matches(field, &self.field, "pattern")` |
//! | `#[validate(custom(predicate = "fn", message = "msg"))]` | `validate_custom(field, &self.field, fn, "msg")` |

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

use crate::rules::{emit_call, parse_validate_attrs};

pub fn expand_validatable(input: DeriveInput) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "#[derive(Validatable)] only supports structs with named fields",
                )
                .to_compile_error()
            }
        },
        _ => {
            return syn::Error::new_spanned(
                name,
                "#[derive(Validatable)] only supports structs",
            )
            .to_compile_error()
        }
    };

    let mut call_exprs: Vec<TokenStream> = Vec::new();

    for field in fields {
        let field_ident = field.ident.as_ref().unwrap();
        let field_name = field_ident.to_string();

        let ref_expr = quote!(&self.#field_ident);
        let direct_expr = quote!(self.#field_ident);

        let rules = match parse_validate_attrs(&field.attrs) {
            Ok(r) => r,
            Err(e) => return e.to_compile_error(),
        };

        for rule in &rules {
            match emit_call(rule, &field_name, &ref_expr, &direct_expr) {
                Ok(ts) => call_exprs.push(ts),
                Err(e) => return e.to_compile_error(),
            }
        }
    }

    if call_exprs.is_empty() {
        return quote! {
            impl #impl_generics ::rust_api::validation::Validatable for #name #ty_generics #where_clause {
                fn validate(self) -> ::std::result::Result<Self, ::std::vec::Vec<::rust_api::validation::FieldError>> {
                    ::std::result::Result::Ok(self)
                }
            }
        };
    }

    let first = &call_exprs[0];
    let rest = &call_exprs[1..];

    quote! {
        impl #impl_generics ::rust_api::validation::Validatable for #name #ty_generics #where_clause {
            fn validate(self) -> ::std::result::Result<Self, ::std::vec::Vec<::rust_api::validation::FieldError>> {
                use ::rust_api::validation::IntoValidated;
                let checks = #first.into_validated()
                    #(+ #rest)*;
                checks.into_result().map(|_| self)
            }
        }
    }
}
