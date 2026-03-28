use std::collections::BTreeMap;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Error, Ident, Result, Token, TypePath, Variant, Visibility, braced,
    parse_macro_input,
};

use crate::shared::codegen::enum_impl_helpers;
use crate::shared::constructors::gen_variant_ctors_simple;
use crate::shared::derive::merge_debug_derive;
use crate::shared::display::display_arm;
use crate::shared::from_attr::{from_variant_source, is_from_variant};
use crate::shared::options::parse_diagweave_options;
use crate::shared::sanitize::sanitize_variant_attrs;

pub(crate) fn union_impl(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as UnionInput);
    match expand_union(parsed) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_union(input: UnionInput) -> Result<proc_macro2::TokenStream> {
    let options = parse_diagweave_options(&input.attrs)?;
    let attrs = strip_union_attrs(input.attrs);
    let mut generated_variants = Vec::new();
    let mut from_impls = Vec::new();
    let mut display_arms = Vec::new();
    let mut constructor_variants = Vec::new();
    let mut used_variant_names = BTreeMap::<String, Span>::new();
    let mut used_source_types = BTreeMap::<String, Span>::new();
    let enum_name = &input.name;
    let vis = input.vis;

    let mut ctx = ExpandContext {
        generated_variants: &mut generated_variants,
        from_impls: &mut from_impls,
        display_arms: &mut display_arms,
        constructor_variants: &mut constructor_variants,
        used_variant_names: &mut used_variant_names,
        used_source_types: &mut used_source_types,
        enum_name,
    };
    for term in input.terms {
        ctx.expand_term(term)?;
    }
    let constructors = gen_variant_ctors_simple(
        enum_name,
        &constructor_variants,
        &options.report_path,
        &options.constructor_prefix,
    )?;

    let merged_attrs = merge_debug_derive(attrs)?;
    let enum_impl_helpers = enum_impl_helpers(enum_name);
    Ok(quote! {
        #(#merged_attrs)*
        #vis enum #enum_name {
            #(#generated_variants),*
        }

        impl #enum_name {
            #(#constructors)*
        }
        #enum_impl_helpers

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        #(#from_impls)*
    })
}

fn strip_union_attrs(attrs: Vec<Attribute>) -> Vec<Attribute> {
    attrs
        .into_iter()
        .filter(|attr| !attr.path().is_ident("union") && !attr.path().is_ident("diagweave"))
        .collect()
}

fn check_unique_variant(
    ident: &Ident,
    used: &mut BTreeMap<String, Span>,
    span: Span,
) -> Result<()> {
    let key = ident.to_string();
    if used.contains_key(&key) {
        return Err(Error::new(
            span,
            format!("duplicate variant name `{key}` in union!"),
        ));
    }
    used.insert(key, span);
    Ok(())
}

struct UnionInput {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    terms: Vec<UnionItem>,
}

impl Parse for UnionInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse::<Visibility>()?;
        input.parse::<Token![enum]>()?;
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let terms = Punctuated::<UnionItem, Token![|]>::parse_separated_nonempty(input)?;
        Ok(Self {
            attrs,
            vis,
            name,
            terms: terms.into_iter().collect(),
        })
    }
}

enum UnionItem {
    External { ty: TypePath, alias: Option<Ident> },
    Inline(InlineVariants),
}

impl Parse for UnionItem {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(syn::token::Brace) {
            return Ok(Self::Inline(input.parse::<InlineVariants>()?));
        }
        let ty = input.parse::<TypePath>()?;
        let alias = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse::<Ident>()?)
        } else {
            None
        };
        Ok(Self::External { ty, alias })
    }
}

#[derive(Clone)]
struct InlineVariants {
    variants: Vec<Variant>,
}

impl Parse for InlineVariants {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let content;
        braced!(content in input);
        let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
        Ok(Self {
            variants: variants.into_iter().collect(),
        })
    }
}

struct ExpandContext<'a> {
    generated_variants: &'a mut Vec<proc_macro2::TokenStream>,
    from_impls: &'a mut Vec<proc_macro2::TokenStream>,
    display_arms: &'a mut Vec<proc_macro2::TokenStream>,
    constructor_variants: &'a mut Vec<Variant>,
    used_variant_names: &'a mut BTreeMap<String, Span>,
    used_source_types: &'a mut BTreeMap<String, Span>,
    enum_name: &'a Ident,
}

impl<'a> ExpandContext<'a> {
    fn expand_term(&mut self, term: UnionItem) -> Result<()> {
        match term {
            UnionItem::External { ty, alias } => self.expand_external(ty, alias),
            UnionItem::Inline(inline) => self.expand_inline_item(inline),
        }
    }

    fn expand_external(&mut self, ty: syn::TypePath, alias: Option<Ident>) -> Result<()> {
        let enum_name = self.enum_name;
        let variant_ident = alias.unwrap_or_else(|| {
            let last = ty.path.segments.last().map(|s| &s.ident);
            match last {
                Some(id) => id.clone(),
                None => Ident::new("Unknown", Span::call_site()),
            }
        });
        check_unique_variant(
            &variant_ident,
            self.used_variant_names,
            variant_ident.span(),
        )?;
        self.generated_variants.push(quote! { #variant_ident(#ty) });
        self.display_arms.push(quote! {
            #enum_name::#variant_ident(inner) => write!(f, "{}", inner)
        });
        self.constructor_variants
            .push(syn::parse_quote!(#variant_ident(#ty)));
        let key = quote::quote!(#ty).to_string();
        if let Some(_previous) = self.used_source_types.get(&key) {
            return Err(Error::new(
                variant_ident.span(),
                format!("duplicate From source type `{}` in `{}`", key, enum_name),
            ));
        }
        self.used_source_types.insert(key, variant_ident.span());
        self.from_impls.push(quote! {
            impl ::core::convert::From<#ty> for #enum_name {
                fn from(value: #ty) -> Self { Self::#variant_ident(value) }
            }
        });
        Ok(())
    }

    fn expand_inline_item(&mut self, inline: InlineVariants) -> Result<()> {
        let enum_name = self.enum_name;
        for variant in inline.variants {
            check_unique_variant(
                &variant.ident,
                self.used_variant_names,
                variant.ident.span(),
            )?;
            self.display_arms.push(display_arm(enum_name, &variant)?);
            self.constructor_variants.push(variant.clone());
            if is_from_variant(&variant)? {
                let (source_ty, ctor) = from_variant_source(enum_name, &variant)?;
                let key = quote::quote!(#source_ty).to_string();
                if let Some(_previous) = self.used_source_types.get(&key) {
                    return Err(Error::new(
                        variant.ident.span(),
                        format!("duplicate From source type `{}` in `{}`", key, enum_name),
                    ));
                }
                self.used_source_types.insert(key, variant.ident.span());
                self.from_impls.push(quote! {
                    impl ::core::convert::From<#source_ty> for #enum_name {
                        fn from(value: #source_ty) -> Self {
                            #ctor
                        }
                    }
                });
            }
            let variant = sanitize_variant_attrs(&variant);
            self.generated_variants.push(quote! { #variant });
        }
        Ok(())
    }
}
