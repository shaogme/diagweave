use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::BTreeMap;
use syn::{
    Attribute, Error, Fields, Ident, LitStr, Path, Result, Token, Type, Variant,
    punctuated::Punctuated,
};

use crate::set::parser::SetOptions;
use crate::set::resolver::{ResolvedSet, ResolvedVariant};

pub(crate) fn generate_enum_impl(set: &ResolvedSet, options: &SetOptions) -> Result<TokenStream> {
    let enum_ident = &set.name;
    let variants: Vec<Variant> = set
        .variants
        .iter()
        .map(|v| sanitize_variant(&v.variant))
        .collect();
    let display_arms = set
        .variants
        .iter()
        .map(|v| display_arm(enum_ident, &v.variant))
        .collect::<Result<Vec<_>>>()?;
    let constructors = variant_constructors(
        enum_ident,
        &set.variants,
        &options.report_path,
        &options.constructor_prefix,
    )?;
    let variant_from_impls = from_impls_for_variants(enum_ident, &set.variants)?;
    let merged_attrs = merge_debug_derive(set.attrs.clone())?;
    Ok(quote! {
        #(#merged_attrs)*
        pub enum #enum_ident { #(#variants),* }
        impl #enum_ident {
            #(#constructors)*
            pub fn diag(self) -> ::diagweave::report::Report<Self> { ::diagweave::report::Report::new(self) }
            pub fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                <Self as ::core::error::Error>::source(self)
            }
            pub fn diag_with<C>(self) -> ::diagweave::report::Report<Self, C> where C: ::diagweave::report::CauseStore {
                ::diagweave::report::Report::<Self, C>::new_with_store(self)
            }
        }
        impl ::core::fmt::Display for #enum_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self { #(#display_arms),* }
            }
        }
        impl ::core::error::Error for #enum_ident {}
        #(#variant_from_impls)*
    })
}

pub(crate) fn generate_all_from_impls(
    names: &[String],
    resolved: &BTreeMap<String, ResolvedSet>,
) -> Result<Vec<TokenStream>> {
    let mut from_impls = Vec::new();
    for outer_name in names {
        let outer = resolved
            .get(outer_name)
            .ok_or_else(|| Error::new(Span::call_site(), "resolved set must exist"))?;
        for inner_name in names {
            if inner_name == outer_name {
                continue;
            }
            let inner = resolved
                .get(inner_name)
                .ok_or_else(|| Error::new(Span::call_site(), "resolved set must exist"))?;
            if inner.members.is_subset_of(&outer.members) {
                let arms = inner
                    .variants
                    .iter()
                    .map(|v| from_arm(&inner.name, &outer.name, &v.variant))
                    .collect::<Result<Vec<_>>>()?;
                let inner_ident = &inner.name;
                let outer_ident = &outer.name;
                from_impls.push(quote! {
                    impl ::core::convert::From<#inner_ident> for #outer_ident {
                        fn from(value: #inner_ident) -> Self {
                            match value {
                                #(#arms),*
                            }
                        }
                    }
                });
            }
        }
    }
    Ok(from_impls)
}

fn sanitize_variant(variant: &Variant) -> Variant {
    let mut sanitized = variant.clone();
    sanitized.attrs = sanitize_attrs(&sanitized.attrs);
    sanitized
}

fn sanitize_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("display") && !attr.path().is_ident("from"))
        .cloned()
        .collect()
}

fn display_arm(enum_ident: &Ident, variant: &Variant) -> Result<TokenStream> {
    let variant_name = &variant.ident;
    let display = parse_display_mode(&variant.attrs)?;
    match &variant.fields {
        Fields::Unit => display_arm_unit(enum_ident, variant_name, display),
        Fields::Named(named) => display_arm_named(enum_ident, variant_name, display, named),
        Fields::Unnamed(unnamed) => display_arm_unnamed(enum_ident, variant_name, display, unnamed),
    }
}

fn display_arm_unit(
    enum_ident: &Ident,
    variant_name: &Ident,
    display: DisplayMode,
) -> Result<TokenStream> {
    if matches!(display, DisplayMode::Transparent) {
        return Err(Error::new(
            variant_name.span(),
            "#[display(transparent)] requires exactly one tuple field",
        ));
    }
    let expr = display_expr(enum_ident, variant_name, &display, &[])?;
    Ok(quote! { #enum_ident::#variant_name => { #expr } })
}

fn display_arm_named(
    enum_ident: &Ident,
    variant_name: &Ident,
    display: DisplayMode,
    named: &syn::FieldsNamed,
) -> Result<TokenStream> {
    if matches!(display, DisplayMode::Transparent) {
        return Err(Error::new(
            variant_name.span(),
            "#[display(transparent)] requires exactly one tuple field",
        ));
    }
    let idents = named
        .named
        .iter()
        .map(|field| {
            field
                .ident
                .clone()
                .ok_or_else(|| Error::new_spanned(field, "named field should have ident"))
        })
        .collect::<Result<Vec<_>>>()?;
    let replacements = idents
        .iter()
        .map(|ident| (ident.to_string(), quote! { #ident }))
        .collect::<Vec<_>>();
    let expr = display_expr(enum_ident, variant_name, &display, &replacements)?;
    Ok(quote! { #enum_ident::#variant_name { #(#idents),* } => { #expr } })
}

fn display_arm_unnamed(
    enum_ident: &Ident,
    variant_name: &Ident,
    display: DisplayMode,
    unnamed: &syn::FieldsUnnamed,
) -> Result<TokenStream> {
    if matches!(display, DisplayMode::Transparent) && unnamed.unnamed.len() != 1 {
        return Err(Error::new(
            variant_name.span(),
            "#[display(transparent)] requires exactly one tuple field",
        ));
    }
    let binders = (0..unnamed.unnamed.len())
        .map(|idx| format_ident!("f{idx}"))
        .collect::<Vec<_>>();
    let replacements = binders
        .iter()
        .enumerate()
        .map(|(idx, ident)| (idx.to_string(), quote! { #ident }))
        .collect::<Vec<_>>();
    let expr = display_expr(enum_ident, variant_name, &display, &replacements)?;
    Ok(quote! { #enum_ident::#variant_name(#(#binders),*) => { #expr } })
}

#[derive(Clone)]
enum DisplayMode {
    Default,
    Template(LitStr),
    Transparent,
}

fn parse_display_mode(attrs: &[Attribute]) -> Result<DisplayMode> {
    let mut mode = DisplayMode::Default;
    for attr in attrs {
        if !attr.path().is_ident("display") {
            continue;
        }
        let parsed = if let Ok(lit) = attr.parse_args::<LitStr>() {
            DisplayMode::Template(lit)
        } else {
            let ident = attr.parse_args::<Ident>()?;
            if ident == "transparent" {
                DisplayMode::Transparent
            } else {
                return Err(Error::new_spanned(
                    ident,
                    "unsupported #[display(...)] argument; expected string literal or `transparent`",
                ));
            }
        };
        if !matches!(mode, DisplayMode::Default) {
            return Err(Error::new_spanned(
                attr,
                "duplicate #[display(...)] on variant",
            ));
        }
        mode = parsed;
    }
    Ok(mode)
}

fn display_expr(
    enum_ident: &Ident,
    variant_name: &Ident,
    display: &DisplayMode,
    replacements: &[(String, TokenStream)],
) -> Result<TokenStream> {
    match display {
        DisplayMode::Template(template_lit) => {
            let (fmt_template, ordered_tokens) =
                render_display_template(template_lit, replacements)?;
            Ok(quote! {
                write!(f, #fmt_template #(, #ordered_tokens)*)
            })
        }
        DisplayMode::Transparent => {
            if replacements.len() != 1 {
                return Err(Error::new(
                    variant_name.span(),
                    "#[display(transparent)] requires exactly one tuple field",
                ));
            }
            let inner = &replacements[0].1;
            Ok(quote! {
                write!(f, "{}", #inner)
            })
        }
        DisplayMode::Default => Ok(quote! {
            write!(f, "{}::{}", stringify!(#enum_ident), stringify!(#variant_name))
        }),
    }
}

fn render_display_template(
    template: &LitStr,
    replacements: &[(String, TokenStream)],
) -> Result<(String, Vec<TokenStream>)> {
    let mut output = String::new();
    let mut ordered = Vec::new();
    let raw = template.value();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                let (part, new_i) =
                    parse_brace_open(template, &chars, i, replacements, &mut ordered)?;
                output.push_str(&part);
                i = new_i;
            }
            '}' => {
                let (part, new_i) = parse_brace_close(template, &chars, i)?;
                output.push_str(&part);
                i = new_i;
            }
            ch => {
                output.push(ch);
                i += 1;
            }
        }
    }
    Ok((output, ordered))
}

fn variant_constructors(
    enum_ident: &Ident,
    variants: &[ResolvedVariant],
    report_path: &Path,
    constructor_prefix: &str,
) -> Result<Vec<TokenStream>> {
    let mut used = BTreeMap::<String, Span>::new();
    let mut constructors = Vec::new();
    for resolved in variants {
        let variant = &resolved.variant;
        let base_name = to_snake_case(&variant.ident.to_string());
        let ctor_name = if constructor_prefix.is_empty() {
            base_name
        } else {
            format!("{constructor_prefix}_{base_name}")
        };
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

fn expand_constructor_fields(fields: &Fields) -> Result<(Vec<TokenStream>, TokenStream)> {
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

fn from_impls_for_variants(
    enum_ident: &Ident,
    variants: &[ResolvedVariant],
) -> Result<Vec<TokenStream>> {
    let mut used_source_types = BTreeMap::<String, Span>::new();
    let mut impls = Vec::new();
    for resolved in variants {
        let variant = &resolved.variant;
        if !has_from_attr(&variant.attrs)? {
            continue;
        }
        let (source_ty, ctor) = from_variant_source(enum_ident, variant)?;
        let key = quote!(#source_ty).to_string();
        if let Some(previous) = used_source_types.get(&key) {
            return Err(Error::new(
                variant.ident.span(),
                format!(
                    "duplicate #[from] source type `{}` in `{}`; previous #[from] span: {:?}",
                    key, enum_ident, previous
                ),
            ));
        }
        used_source_types.insert(key, variant.ident.span());
        impls.push(quote! {
            impl ::core::convert::From<#source_ty> for #enum_ident {
                fn from(value: #source_ty) -> Self {
                    #ctor
                }
            }
        });
    }
    Ok(impls)
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

fn from_variant_source(enum_ident: &Ident, variant: &Variant) -> Result<(Type, TokenStream)> {
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

fn from_arm(inner: &Ident, outer: &Ident, variant: &Variant) -> Result<TokenStream> {
    let variant_name = &variant.ident;
    match &variant.fields {
        Fields::Unit => Ok(quote! {
            #inner::#variant_name => #outer::#variant_name
        }),
        Fields::Named(fields_named) => {
            let idents: Vec<Ident> = fields_named
                .named
                .iter()
                .map(|f| {
                    f.ident
                        .clone()
                        .ok_or_else(|| Error::new_spanned(f, "named field should have ident"))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(quote! {
                #inner::#variant_name { #(#idents),* } => #outer::#variant_name { #(#idents),* }
            })
        }
        Fields::Unnamed(fields_unnamed) => {
            let binders = (0..fields_unnamed.unnamed.len())
                .map(|idx| format_ident!("f{idx}"))
                .collect::<Vec<_>>();
            Ok(quote! {
                #inner::#variant_name(#(#binders),*) => #outer::#variant_name(#(#binders),*)
            })
        }
    }
}

pub(crate) fn merge_debug_derive(attrs: Vec<Attribute>) -> Result<Vec<Attribute>> {
    let mut derive_paths = Vec::<Path>::new();
    let mut passthrough = Vec::<Attribute>::new();
    let mut seen = BTreeMap::<String, ()>::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let parsed = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
            for path in parsed {
                let key = quote::quote!(#path).to_string();
                if seen.insert(key, ()).is_none() {
                    derive_paths.push(path);
                }
            }
        } else {
            passthrough.push(attr);
        }
    }
    if !derive_paths.iter().any(|path| path.is_ident("Debug")) {
        derive_paths.push(syn::parse_quote!(Debug));
    }
    let mut merged = Vec::new();
    merged.push(syn::parse_quote!(#[derive(#(#derive_paths),*)]));
    merged.extend(passthrough);
    Ok(merged)
}

fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = input.chars().collect();
    for (idx, ch) in chars.iter().enumerate() {
        let is_upper = ch.is_ascii_uppercase();
        if is_upper {
            if idx > 0 {
                let prev = chars[idx - 1];
                let next_is_lower = chars
                    .get(idx + 1)
                    .map(|next| next.is_ascii_lowercase())
                    .unwrap_or(false);
                if prev.is_ascii_lowercase() || next_is_lower {
                    out.push('_');
                }
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(*ch);
        }
    }
    out
}

fn parse_brace_open(
    template: &LitStr,
    chars: &[char],
    i: usize,
    replacements: &[(String, TokenStream)],
    ordered: &mut Vec<TokenStream>,
) -> Result<(String, usize)> {
    if i + 1 < chars.len() && chars[i + 1] == '{' {
        return Ok(("{{".to_string(), i + 2));
    }
    let start = i + 1;
    let mut end = start;
    while end < chars.len() && chars[end] != '}' {
        end += 1;
    }
    if end >= chars.len() {
        return Err(Error::new_spanned(
            template,
            "unclosed `{` in #[display(...)] template",
        ));
    }
    let key: String = chars[start..end].iter().collect();
    if key.is_empty() {
        return Err(Error::new_spanned(
            template,
            "empty `{}` placeholder is not allowed in #[display(...)] template",
        ));
    }
    if let Some((_, token)) = replacements.iter().find(|(name, _)| name == &key) {
        ordered.push(token.clone());
        Ok(("{}".to_string(), end + 1))
    } else {
        Err(Error::new_spanned(
            template,
            format!(
                "unknown placeholder `{{{key}}}` in #[display(...)] template; placeholders come from named fields or zero-based tuple indices"
            ),
        ))
    }
}

fn parse_brace_close(template: &LitStr, chars: &[char], i: usize) -> Result<(String, usize)> {
    if i + 1 < chars.len() && chars[i + 1] == '}' {
        Ok(("}}".to_string(), i + 2))
    } else {
        Err(Error::new_spanned(
            template,
            "unmatched `}` in #[display(...)] template",
        ))
    }
}
