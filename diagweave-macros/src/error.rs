pub(crate) mod codegen;
pub(crate) mod display;
pub(crate) mod source;

use std::collections::BTreeMap;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataEnum, DataStruct, DeriveInput, Error, Ident, Result};

pub(crate) fn derive_error_impl(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    match expand_derive_error(parsed) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_derive_error(input: DeriveInput) -> Result<proc_macro2::TokenStream> {
    let ident = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let (display_impl, error_source_fn, from_impls) = match &input.data {
        Data::Enum(data) => expand_enum(&ident, data)?,
        Data::Struct(data) => expand_struct(&ident, &input.attrs, data)?,
        Data::Union(_) => {
            return Err(Error::new_spanned(
                &ident,
                "#[derive(Error)] only supports structs and enums",
            ));
        }
    };

    Ok(quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub fn diag(self) -> ::diagweave::report::Report<Self> {
                ::diagweave::report::Report::new(self)
            }

            pub fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                <Self as ::core::error::Error>::source(self)
            }
        }

        impl #impl_generics ::core::fmt::Display for #ident #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #display_impl
            }
        }

        impl #impl_generics ::core::error::Error for #ident #ty_generics #where_clause {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                #error_source_fn
            }
        }

        #(#from_impls)*
    })
}

fn expand_enum(
    enum_ident: &Ident,
    data: &DataEnum,
) -> Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    Vec<proc_macro2::TokenStream>,
)> {
    let mut display_arms = Vec::new();
    let mut source_arms = Vec::new();
    let mut from_impls = Vec::new();
    let mut from_types = BTreeMap::<String, proc_macro2::Span>::new();

    for variant in &data.variants {
        let (display_arm, source_arm, from_impl) =
            expand_variant(enum_ident, variant, &mut from_types)?;
        display_arms.push(display_arm);
        source_arms.push(source_arm);
        if let Some(fi) = from_impl {
            from_impls.push(fi);
        }
    }

    Ok((
        quote! {
            match self {
                #(#display_arms),*
            }
        },
        quote! {
            match self {
                #(#source_arms),*
            }
        },
        from_impls,
    ))
}

fn expand_struct(
    ident: &Ident,
    attrs: &[Attribute],
    data: &DataStruct,
) -> Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    Vec<proc_macro2::TokenStream>,
)> {
    let display = display::parse_error_display(attrs, ident.span())?;
    let binding = codegen::make_bindings(&data.fields)?;
    let replacements = display::replacements(&data.fields, &binding)?;
    let display_expr = display::display_expr(&display, ident, &replacements)?;
    let source_expr = source::source_expr_for_struct(&data.fields, &display)?;

    let mut from_impls = Vec::new();
    if let Some(from_field) = source::from_field(&data.fields)? {
        let ty = source::field_type(&data.fields, from_field)?;
        let ctor = codegen::struct_ctor(ident, &data.fields, from_field)?;
        from_impls.push(quote! {
            impl ::core::convert::From<#ty> for #ident {
                fn from(value: #ty) -> Self {
                    #ctor
                }
            }
        });
    }

    let pattern = codegen::struct_pattern(ident, &data.fields, &binding);
    Ok((
        quote! {
            match self {
                #pattern => { #display_expr }
            }
        },
        quote! {
            #source_expr
        },
        from_impls,
    ))
}

fn expand_variant(
    enum_ident: &Ident,
    variant: &syn::Variant,
    from_types: &mut BTreeMap<String, proc_macro2::Span>,
) -> Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    Option<proc_macro2::TokenStream>,
)> {
    let display = display::parse_error_display(&variant.attrs, variant.span())?;
    let fields = &variant.fields;
    let binding = codegen::make_bindings(fields)?;
    let pattern = codegen::variant_pattern(enum_ident, &variant.ident, fields, &binding);
    let replacements = display::replacements(fields, &binding)?;
    let display_expr = display::display_expr(&display, &variant.ident, &replacements)?;
    let display_arm = quote! { #pattern => { #display_expr } };
    let source_arm = source::source_arm_for_variant(&variant.ident, fields, &display)?;

    let mut from_impl = None;
    if let Some(from_field) = source::from_field(fields)? {
        let ty = source::field_type(fields, from_field)?;
        let key = quote!(#ty).to_string();
        if let Some(prev) = from_types.get(&key) {
            return Err(Error::new(
                variant.ident.span(),
                format!(
                    "duplicate #[from] source type `{key}` in `{enum_ident}`; previous #[from] span: {prev:?}"
                ),
            ));
        }
        from_types.insert(key, variant.ident.span());
        let ctor = codegen::variant_ctor(enum_ident, &variant.ident, fields, from_field)?;
        from_impl = Some(quote! {
            impl ::core::convert::From<#ty> for #enum_ident {
                fn from(value: #ty) -> Self { #ctor }
            }
        });
    }

    Ok((display_arm, source_arm, from_impl))
}
