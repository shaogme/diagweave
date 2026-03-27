use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Fields, Ident, Result, Type, Variant};

pub(crate) fn is_from_variant(variant: &Variant) -> Result<bool> {
    let mut count = 0;
    if has_from_attr(&variant.attrs)? {
        count += 1;
    }
    for field in variant.fields.iter() {
        if has_from_attr(&field.attrs)? {
            count += 1;
        }
    }
    if count > 1 {
        return Err(Error::new_spanned(
            variant,
            "duplicate #[from] on variant or its fields",
        ));
    }
    Ok(count == 1)
}

fn has_from_attr(attrs: &[Attribute]) -> Result<bool> {
    let mut has = false;
    for attr in attrs {
        if !attr.path().is_ident("from") {
            continue;
        }
        if has {
            return Err(Error::new_spanned(attr, "duplicate #[from] on variant"));
        }
        if attr.meta.require_path_only().is_err() {
            return Err(Error::new_spanned(
                attr,
                "#[from] does not accept arguments",
            ));
        }
        has = true;
    }
    Ok(has)
}

pub(crate) fn from_variant_source(
    enum_ident: &Ident,
    variant: &Variant,
) -> Result<(Type, TokenStream)> {
    let variant_ident = &variant.ident;
    match &variant.fields {
        Fields::Unnamed(fields_unnamed) if fields_unnamed.unnamed.len() == 1 => {
            let source_ty = fields_unnamed
                .unnamed
                .first()
                .ok_or_else(|| Error::new_spanned(variant, "no fields in unnamed variant"))?
                .ty
                .clone();
            Ok((
                source_ty,
                quote! {
                    #enum_ident::#variant_ident(value)
                },
            ))
        }
        _ => Err(Error::new_spanned(
            variant,
            "#[from] requires exactly one tuple field variant, e.g. Variant(Source)",
        )),
    }
}
