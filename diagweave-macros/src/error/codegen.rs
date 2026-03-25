use quote::{format_ident, quote};
use syn::{Error, Fields, Ident, Result};

#[derive(Clone)]
pub(crate) enum BindingStyle {
    Unit,
    Named(Vec<Ident>),
    Unnamed(Vec<Ident>),
}

pub(crate) fn make_bindings(fields: &Fields) -> Result<BindingStyle> {
    use syn::spanned::Spanned;
    match fields {
        Fields::Unit => Ok(BindingStyle::Unit),
        Fields::Named(named) => {
            let mut idents = Vec::new();
            for field in &named.named {
                let id = field
                    .ident
                    .as_ref()
                    .ok_or_else(|| Error::new(field.span(), "named field should have ident"))?
                    .clone();
                idents.push(id);
            }
            Ok(BindingStyle::Named(idents))
        }
        Fields::Unnamed(unnamed) => Ok(BindingStyle::Unnamed(
            (0..unnamed.unnamed.len())
                .map(|i| format_ident!("f{i}"))
                .collect(),
        )),
    }
}

pub(crate) fn variant_pattern(
    enum_ident: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
    binding: &BindingStyle,
) -> proc_macro2::TokenStream {
    match (fields, binding) {
        (Fields::Unit, BindingStyle::Unit) => quote! { Self::#variant_ident },
        (Fields::Named(_), BindingStyle::Named(idents)) => {
            quote! { Self::#variant_ident { #(#idents),* } }
        }
        (Fields::Unnamed(_), BindingStyle::Unnamed(idents)) => {
            quote! { Self::#variant_ident(#(#idents),*) }
        }
        _ => quote! { #enum_ident::#variant_ident },
    }
}

pub(crate) fn struct_pattern(
    ident: &Ident,
    fields: &Fields,
    binding: &BindingStyle,
) -> proc_macro2::TokenStream {
    match (fields, binding) {
        (Fields::Unit, BindingStyle::Unit) => quote! { Self },
        (Fields::Named(_), BindingStyle::Named(idents)) => quote! { Self { #(#idents),* } },
        (Fields::Unnamed(_), BindingStyle::Unnamed(idents)) => quote! { Self(#(#idents),*) },
        _ => quote! { #ident },
    }
}

pub(crate) fn variant_ctor(
    enum_ident: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
    from_index: usize,
) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    match fields {
        Fields::Named(named) => {
            let field_ident = named
                .named
                .iter()
                .nth(from_index)
                .and_then(|field| field.ident.clone())
                .ok_or_else(|| Error::new(fields.span(), "invalid #[from] field index"))?;
            Ok(quote! {
                #enum_ident::#variant_ident { #field_ident: value }
            })
        }
        Fields::Unnamed(_) => Ok(quote! {
            #enum_ident::#variant_ident(value)
        }),
        Fields::Unit => Err(Error::new(
            fields.span(),
            "#[from] requires a field-bearing variant",
        )),
    }
}

pub(crate) fn struct_ctor(
    ident: &Ident,
    fields: &Fields,
    from_index: usize,
) -> Result<proc_macro2::TokenStream> {
    use syn::spanned::Spanned;
    match fields {
        Fields::Named(named) => {
            let field_ident = named
                .named
                .iter()
                .nth(from_index)
                .and_then(|field| field.ident.clone())
                .ok_or_else(|| Error::new(fields.span(), "invalid #[from] field index"))?;
            Ok(quote! {
                #ident { #field_ident: value }
            })
        }
        Fields::Unnamed(_) => Ok(quote! { #ident(value) }),
        Fields::Unit => Err(Error::new(
            fields.span(),
            "#[from] requires a field-bearing struct",
        )),
    }
}

pub(crate) fn field_len(fields: &Fields) -> usize {
    match fields {
        Fields::Unit => 0,
        Fields::Named(named) => named.named.len(),
        Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
    }
}
