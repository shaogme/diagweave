use quote::quote;
use syn::{Fields, Ident, Result};

#[derive(Clone)]
pub(crate) struct FieldRef {
    pub(crate) index: usize,
    pub(crate) has_from: bool,
    pub(crate) has_source: bool,
}

pub(crate) fn from_field(fields: &Fields) -> Result<Option<usize>> {
    use syn::spanned::Spanned;
    let refs = scan_field_refs(fields)?;
    let mut found = None;
    for field in refs {
        if field.has_from && found.replace(field.index).is_some() {
            return Err(syn::Error::new(
                fields.span(),
                "multiple #[from] fields are not supported",
            ));
        }
    }
    if let Some(idx) = found {
        if super::codegen::field_len(fields) != 1 {
            return Err(syn::Error::new(
                fields.span(),
                "#[from] requires exactly one field",
            ));
        }
        return Ok(Some(idx));
    }
    Ok(None)
}

pub(crate) fn source_field(fields: &Fields) -> Result<Option<FieldRef>> {
    use syn::spanned::Spanned;
    let refs = scan_field_refs(fields)?;
    let mut found: Option<FieldRef> = None;
    for field in refs {
        if (field.has_source || field.has_from) && found.replace(field.clone()).is_some() {
            return Err(syn::Error::new(
                fields.span(),
                "multiple #[source]/#[from] fields are not supported",
            ));
        }
    }
    Ok(found)
}

fn scan_field_refs(fields: &Fields) -> Result<Vec<FieldRef>> {
    let mut refs = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let mut has_from = false;
        let mut has_source = false;
        for attr in &field.attrs {
            if attr.path().is_ident("from") {
                if has_from {
                    return Err(syn::Error::new_spanned(attr, "duplicate #[from] on field"));
                }
                if attr.meta.require_path_only().is_err() {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "#[from] does not accept arguments",
                    ));
                }
                has_from = true;
            }
            if attr.path().is_ident("source") {
                if has_source {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "duplicate #[source] on field",
                    ));
                }
                if attr.meta.require_path_only().is_err() {
                    return Err(syn::Error::new_spanned(
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

pub(crate) fn field_type(fields: &Fields, index: usize) -> Result<syn::Type> {
    use syn::spanned::Spanned;
    fields
        .iter()
        .nth(index)
        .map(|field| field.ty.clone())
        .ok_or_else(|| syn::Error::new(fields.span(), "invalid field index"))
}

pub(crate) fn resolved_source_index(
    fields: &Fields,
    display: &super::display::ErrorDisplay,
) -> Result<Option<usize>> {
    use syn::spanned::Spanned;
    if let Some(source) = source_field(fields)? {
        return Ok(Some(source.index));
    }
    if matches!(display, super::display::ErrorDisplay::Transparent) {
        if super::codegen::field_len(fields) != 1 {
            return Err(syn::Error::new(
                fields.span(),
                "#[display(transparent)] requires exactly one field",
            ));
        }
        return Ok(Some(0));
    }
    Ok(None)
}

pub(crate) fn source_arm_for_variant(
    variant_ident: &Ident,
    fields: &Fields,
    display: &super::display::ErrorDisplay,
) -> Result<proc_macro2::TokenStream> {
    let source_index = resolved_source_index(fields, display)?;
    match fields {
        Fields::Unit => source_arm_unit(variant_ident, source_index, fields),
        Fields::Named(named) => source_arm_named(variant_ident, source_index, named, fields),
        Fields::Unnamed(unnamed) => source_arm_unnamed(variant_ident, source_index, unnamed),
    }
}

pub(crate) fn source_expr_for_struct(
    fields: &Fields,
    display: &super::display::ErrorDisplay,
) -> Result<proc_macro2::TokenStream> {
    let source_index = resolved_source_index(fields, display)?;
    match fields {
        Fields::Unit => source_expr_unit(source_index, fields),
        Fields::Named(named) => source_expr_named(source_index, named, fields),
        Fields::Unnamed(unnamed) => source_expr_unnamed(source_index, unnamed),
    }
}

fn source_arm_unit(
    ident: &Ident,
    index: Option<usize>,
    fields: &Fields,
) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    if index.is_some() {
        return Err(syn::Error::new(
            fields.span(),
            "#[source]/#[from] requires a field-bearing variant",
        ));
    }
    Ok(quote! { Self::#ident => ::core::option::Option::None })
}

fn source_arm_named(
    ident: &Ident,
    index: Option<usize>,
    named: &syn::FieldsNamed,
    fields: &Fields,
) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    if let Some(index) = index {
        let sid = named
            .named
            .iter()
            .nth(index)
            .and_then(|f| f.ident.clone())
            .ok_or_else(|| syn::Error::new(fields.span(), "invalid source field index"))?;
        Ok(quote! {
            Self::#ident { #sid, .. } => {
                let src: &(dyn ::core::error::Error + 'static) = #sid;
                ::core::option::Option::Some(src)
            }
        })
    } else {
        Ok(quote! { Self::#ident { .. } => ::core::option::Option::None })
    }
}

fn source_arm_unnamed(
    ident: &Ident,
    index: Option<usize>,
    unnamed: &syn::FieldsUnnamed,
) -> Result<proc_macro2::TokenStream> {
    if let Some(index) = index {
        let binders = (0..unnamed.unnamed.len()).map(|idx| {
            if idx == index {
                quote!(source)
            } else {
                quote!(_)
            }
        });
        Ok(quote! {
            Self::#ident(#(#binders),*) => {
                let src: &(dyn ::core::error::Error + 'static) = source;
                ::core::option::Option::Some(src)
            }
        })
    } else {
        Ok(quote! { Self::#ident(..) => ::core::option::Option::None })
    }
}

fn source_expr_unit(index: Option<usize>, fields: &Fields) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    if index.is_some() {
        return Err(syn::Error::new(
            fields.span(),
            "#[source]/#[from] requires a field-bearing struct",
        ));
    }
    Ok(quote! { ::core::option::Option::None })
}

fn source_expr_named(
    index: Option<usize>,
    named: &syn::FieldsNamed,
    fields: &Fields,
) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    if let Some(index) = index {
        let sid = named
            .named
            .iter()
            .nth(index)
            .and_then(|f| f.ident.clone())
            .ok_or_else(|| syn::Error::new(fields.span(), "invalid source field index"))?;
        Ok(quote! {
            match self {
                Self { #sid, .. } => {
                    let src: &(dyn ::core::error::Error + 'static) = #sid;
                    ::core::option::Option::Some(src)
                }
            }
        })
    } else {
        Ok(quote! { ::core::option::Option::None })
    }
}

fn source_expr_unnamed(
    index: Option<usize>,
    unnamed: &syn::FieldsUnnamed,
) -> Result<proc_macro2::TokenStream> {
    if let Some(index) = index {
        let binders = (0..unnamed.unnamed.len()).map(|idx| {
            if idx == index {
                quote!(source)
            } else {
                quote!(_)
            }
        });
        Ok(quote! {
            match self {
                Self(#(#binders),*) => {
                    let src: &(dyn ::core::error::Error + 'static) = source;
                    ::core::option::Option::Some(src)
                }
            }
        })
    } else {
        Ok(quote! { ::core::option::Option::None })
    }
}
