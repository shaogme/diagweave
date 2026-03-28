use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeMap;
use syn::{Error, Fields, Ident, Path, Result, Variant};

use crate::shared::naming::to_snake_case;

pub(crate) fn expand_constructor_fields(
    fields: &Fields,
) -> Result<(Vec<TokenStream>, TokenStream)> {
    match fields {
        Fields::Unit => Ok((vec![], quote! {})),
        Fields::Named(fields_named) => {
            let params = fields_named
                .named
                .iter()
                .map(|field| {
                    let ident = field.ident.as_ref().ok_or_else(|| {
                        Error::new_spanned(field, "named field should have ident")
                    })?;
                    let ty = &field.ty;
                    Ok(quote! { #ident: #ty })
                })
                .collect::<Result<Vec<_>>>()?;
            let idents = fields_named
                .named
                .iter()
                .map(|field| {
                    field
                        .ident
                        .as_ref()
                        .ok_or_else(|| Error::new_spanned(field, "named field should have ident"))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((params, quote! { { #(#idents),* } }))
        }
        Fields::Unnamed(fields_unnamed) => {
            let params = fields_unnamed
                .unnamed
                .iter()
                .enumerate()
                .map(|(idx, field)| {
                    let ident = format_ident!("arg{idx}");
                    let ty = &field.ty;
                    quote! { #ident: #ty }
                })
                .collect::<Vec<_>>();
            let idents = (0..fields_unnamed.unnamed.len()).map(|idx| format_ident!("arg{idx}"));
            Ok((params, quote! { (#(#idents),*) }))
        }
    }
}

pub(crate) fn gen_variant_ctors(
    enum_ident: &Ident,
    variants: &[Variant],
    report_path: &Path,
    constructor_prefix: &str,
) -> Result<Vec<TokenStream>> {
    let mut used = BTreeMap::<String, Span>::new();
    let mut constructors = Vec::new();
    for variant in variants {
        let ctor_name = constructor_name(&variant.ident.to_string(), constructor_prefix);
        if let Some(existing) = used.get(&ctor_name) {
            return Err(Error::new(
                variant.ident.span(),
                format!(
                    "constructor name collision `{}` in `{}`; previous constructor span: {:?}",
                    ctor_name, enum_ident, existing
                ),
            ));
        }
        used.insert(ctor_name.clone(), variant.ident.span());
        constructors.push(gen_v_constructor(variant, report_path, &ctor_name)?);
    }
    Ok(constructors)
}

pub(crate) fn gen_variant_ctors_simple(
    enum_ident: &Ident,
    variants: &[Variant],
    report_path: &Path,
    constructor_prefix: &str,
) -> Result<Vec<TokenStream>> {
    let mut used = BTreeMap::<String, Span>::new();
    let mut constructors = Vec::new();
    for variant in variants {
        let ctor_name = constructor_name(&variant.ident.to_string(), constructor_prefix);
        if let Some(_existing) = used.get(&ctor_name) {
            return Err(Error::new(
                variant.ident.span(),
                format!(
                    "constructor name collision `{}` in `{}`",
                    ctor_name, enum_ident
                ),
            ));
        }
        used.insert(ctor_name.clone(), variant.ident.span());
        constructors.push(gen_v_constructor(variant, report_path, &ctor_name)?);
    }
    Ok(constructors)
}

fn constructor_name(variant_name: &str, constructor_prefix: &str) -> String {
    let base_name = to_snake_case(variant_name);
    if constructor_prefix.is_empty() {
        base_name
    } else {
        format!("{constructor_prefix}_{base_name}")
    }
}

fn gen_v_constructor(
    variant: &Variant,
    report_path: &Path,
    ctor_name: &str,
) -> Result<TokenStream> {
    let ctor_ident = Ident::new(ctor_name, variant.ident.span());
    let ctor_report_ident = Ident::new(&format!("{ctor_name}_report"), variant.ident.span());
    let variant_ident = &variant.ident;
    let (params, fields_gen) = expand_constructor_fields(&variant.fields)?;

    Ok(quote! {
        pub fn #ctor_ident(#(#params),*) -> Self {
            Self::#variant_ident #fields_gen
        }
        pub fn #ctor_report_ident(#(#params),*) -> #report_path<Self> {
            #report_path::new(Self::#variant_ident #fields_gen)
        }
    })
}
