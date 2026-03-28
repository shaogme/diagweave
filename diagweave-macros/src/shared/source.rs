use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, Fields, Ident, Result, Variant};

#[derive(Clone, Copy)]
struct FieldRef {
    index: usize,
    has_from: bool,
    has_source: bool,
}

pub(crate) fn source_arm_for_variant(enum_ident: &Ident, variant: &Variant) -> Result<TokenStream> {
    let source_index = resolved_source_index(&variant.fields)?;
    let variant_ident = &variant.ident;
    match &variant.fields {
        Fields::Unit => {
            if source_index.is_some() {
                return Err(Error::new_spanned(
                    &variant.fields,
                    "#[source]/#[from] requires a field-bearing variant",
                ));
            }
            Ok(quote! { #enum_ident::#variant_ident => ::core::option::Option::None })
        }
        Fields::Named(named) => {
            if let Some(index) = source_index {
                let sid = named
                    .named
                    .iter()
                    .nth(index)
                    .and_then(|f| f.ident.clone())
                    .ok_or_else(|| {
                        Error::new_spanned(&variant.fields, "invalid source field index")
                    })?;
                Ok(quote! {
                    #enum_ident::#variant_ident { #sid, .. } => {
                        let src: &(dyn ::core::error::Error + 'static) = #sid;
                        ::core::option::Option::Some(src)
                    }
                })
            } else {
                Ok(quote! { #enum_ident::#variant_ident { .. } => ::core::option::Option::None })
            }
        }
        Fields::Unnamed(unnamed) => {
            if let Some(index) = source_index {
                let binders = (0..unnamed.unnamed.len()).map(|idx| {
                    if idx == index {
                        quote!(source)
                    } else {
                        quote!(_)
                    }
                });
                Ok(quote! {
                    #enum_ident::#variant_ident(#(#binders),*) => {
                        let src: &(dyn ::core::error::Error + 'static) = source;
                        ::core::option::Option::Some(src)
                    }
                })
            } else {
                Ok(quote! { #enum_ident::#variant_ident(..) => ::core::option::Option::None })
            }
        }
    }
}

fn resolved_source_index(fields: &Fields) -> Result<Option<usize>> {
    let refs = scan_field_refs(fields)?;
    let mut found: Option<FieldRef> = None;
    for field in refs {
        if (field.has_source || field.has_from) && found.replace(field).is_some() {
            return Err(Error::new_spanned(
                fields,
                "multiple #[source]/#[from] fields are not supported",
            ));
        }
    }
    if let Some(field) = found {
        if field.has_from && field_len(fields) != 1 {
            return Err(Error::new_spanned(
                fields,
                "#[from] requires exactly one field",
            ));
        }
        return Ok(Some(field.index));
    }
    Ok(None)
}

fn scan_field_refs(fields: &Fields) -> Result<Vec<FieldRef>> {
    let mut refs = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let mut has_from = false;
        let mut has_source = false;
        for attr in &field.attrs {
            if attr.path().is_ident("from") {
                if has_from {
                    return Err(Error::new_spanned(attr, "duplicate #[from] on field"));
                }
                if attr.meta.require_path_only().is_err() {
                    return Err(Error::new_spanned(
                        attr,
                        "#[from] does not accept arguments",
                    ));
                }
                has_from = true;
            }
            if attr.path().is_ident("source") {
                if has_source {
                    return Err(Error::new_spanned(attr, "duplicate #[source] on field"));
                }
                if attr.meta.require_path_only().is_err() {
                    return Err(Error::new_spanned(
                        attr,
                        "#[source] does not accept arguments",
                    ));
                }
                has_source = true;
            }
        }
        refs.push(FieldRef {
            index,
            has_from,
            has_source,
        });
    }
    Ok(refs)
}

fn field_len(fields: &Fields) -> usize {
    match fields {
        Fields::Unit => 0,
        Fields::Named(named) => named.named.len(),
        Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
    }
}
