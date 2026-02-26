//! Shared parsing for `#[validate(...)]` attributes.
//!
//! `ValidateRule` is the structured representation of a single validation rule.
//! It is parsed from attribute syntax and used by both `#[derive(Validatable)]`
//! and `#[derive(NewType)]` to emit validator call expressions.
//!
//! The structure is intentionally preserved (not collapsed to strings) so that
//! a future schema-codegen pass can walk the same enum to emit OpenAPI properties
//! (e.g. `email` → `"format": "email"`, `min_length = N` → `"minLength": N`).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
    Expr, Ident, Lit, LitStr, Meta, Token,
};

// ---------------------------------------------------------------------------
// ValidateRule — structured representation of one rule inside #[validate(...)]
// ---------------------------------------------------------------------------

pub enum ValidateRule {
    /// `#[validate(email)]` / `#[validate(url)]` / etc.
    Flag(Ident),
    /// `#[validate(min_length = 3)]`
    NameValue { name: Ident, value: Expr },
    /// `#[validate(range(min = 0, max = 100))]`
    Range { min: Expr, max: Expr },
    /// `#[validate(custom(predicate = "fn_name", message = "msg"))]`
    Custom { predicate: LitStr, message: LitStr },
}

impl Parse for ValidateRule {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;

        if input.peek(Token![=]) {
            let _eq: Token![=] = input.parse()?;
            let value: Expr = input.parse()?;
            return Ok(ValidateRule::NameValue { name, value });
        }

        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);

            return match name.to_string().as_str() {
                "range" => {
                    let args: Punctuated<Meta, Comma> =
                        content.parse_terminated(Meta::parse, Token![,])?;
                    let mut min: Option<Expr> = None;
                    let mut max: Option<Expr> = None;
                    for arg in &args {
                        if let Meta::NameValue(nv) = arg {
                            if nv.path.is_ident("min") {
                                min = Some(nv.value.clone());
                            } else if nv.path.is_ident("max") {
                                max = Some(nv.value.clone());
                            }
                        }
                    }
                    Ok(ValidateRule::Range {
                        min: min.ok_or_else(|| content.error("range requires min"))?,
                        max: max.ok_or_else(|| content.error("range requires max"))?,
                    })
                }
                "custom" => {
                    let args: Punctuated<Meta, Comma> =
                        content.parse_terminated(Meta::parse, Token![,])?;
                    let mut predicate: Option<LitStr> = None;
                    let mut message: Option<LitStr> = None;
                    for arg in &args {
                        if let Meta::NameValue(nv) = arg {
                            if nv.path.is_ident("predicate") {
                                if let Expr::Lit(el) = &nv.value {
                                    if let Lit::Str(s) = &el.lit {
                                        predicate = Some(s.clone());
                                    }
                                }
                            } else if nv.path.is_ident("message") {
                                if let Expr::Lit(el) = &nv.value {
                                    if let Lit::Str(s) = &el.lit {
                                        message = Some(s.clone());
                                    }
                                }
                            }
                        }
                    }
                    Ok(ValidateRule::Custom {
                        predicate: predicate
                            .ok_or_else(|| content.error("custom requires predicate"))?,
                        message: message.ok_or_else(|| content.error("custom requires message"))?,
                    })
                }
                other => Err(syn::Error::new_spanned(
                    &name,
                    format!("unknown validate rule `{other}`"),
                )),
            };
        }

        Ok(ValidateRule::Flag(name))
    }
}

// ---------------------------------------------------------------------------
// emit_call — produce a validator call expression
//
// Parameters:
//   rule        — the parsed rule variant
//   field_name  — field name string for the error message (e.g. "email")
//   ref_expr    — expression used for string/custom validators (e.g. `&self.email`
//                 or `&inner`)  — these validators accept `&str` / `&T`
//   direct_expr — expression used for range (e.g. `self.age` or `inner`) — no `&`
// ---------------------------------------------------------------------------

pub fn emit_call(
    rule: &ValidateRule,
    field_name: &str,
    ref_expr: &TokenStream,
    direct_expr: &TokenStream,
) -> Result<TokenStream, syn::Error> {
    match rule {
        ValidateRule::Flag(name) => match name.to_string().as_str() {
            "email" => Ok(quote! {
                ::rust_api::validators::validate_email(#field_name, #ref_expr)
            }),
            "url" => Ok(quote! {
                ::rust_api::validators::validate_url(#field_name, #ref_expr)
            }),
            "uuid" => Ok(quote! {
                ::rust_api::validators::validate_uuid(#field_name, #ref_expr)
            }),
            "non_empty" => Ok(quote! {
                ::rust_api::validators::validate_non_empty(#field_name, #ref_expr)
            }),
            "us_zip" => Ok(quote! {
                ::rust_api::validators::validate_us_zip(#field_name, #ref_expr)
            }),
            "ca_postal" => Ok(quote! {
                ::rust_api::validators::validate_ca_postal(#field_name, #ref_expr)
            }),
            "phone_e164" => Ok(quote! {
                ::rust_api::validators::validate_phone_e164(#field_name, #ref_expr)
            }),
            other => Err(syn::Error::new_spanned(
                name,
                format!("unknown validate flag `{other}`"),
            )),
        },

        ValidateRule::NameValue { name, value } => match name.to_string().as_str() {
            "min_length" => Ok(quote! {
                ::rust_api::validators::validate_min_length(#field_name, #ref_expr, #value)
            }),
            "max_length" => Ok(quote! {
                ::rust_api::validators::validate_max_length(#field_name, #ref_expr, #value)
            }),
            "matches" => Ok(quote! {
                ::rust_api::validators::validate_matches(#field_name, #ref_expr, #value)
            }),
            other => Err(syn::Error::new_spanned(
                name,
                format!("unknown validate key `{other}`"),
            )),
        },

        ValidateRule::Range { min, max } => Ok(quote! {
            ::rust_api::validators::validate_range(#field_name, #direct_expr, #min, #max)
        }),

        ValidateRule::Custom { predicate, message } => {
            let pred_ident = format_ident!("{}", predicate.value());
            Ok(quote! {
                ::rust_api::validators::validate_custom(
                    #field_name,
                    #ref_expr,
                    #pred_ident,
                    #message,
                )
            })
        }
    }
}

// ---------------------------------------------------------------------------
// parse_validate_attrs — collect all ValidateRule items from a slice of attrs
// ---------------------------------------------------------------------------

pub fn parse_validate_attrs(
    attrs: &[syn::Attribute],
) -> Result<Vec<ValidateRule>, syn::Error> {
    let mut rules = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("validate") {
            continue;
        }
        let parsed =
            attr.parse_args_with(Punctuated::<ValidateRule, Comma>::parse_terminated)?;
        rules.extend(parsed);
    }
    Ok(rules)
}
