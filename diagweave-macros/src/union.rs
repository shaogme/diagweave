use std::collections::BTreeMap;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Attribute, Error, Fields, Ident, LitStr, Path, Result, Token, TypePath, Variant, Visibility,
    braced, parse_macro_input,
};

pub(crate) fn union_impl(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as UnionInput);
    match expand_union(parsed) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_union(input: UnionInput) -> Result<proc_macro2::TokenStream> {
    let attrs = strip_union_attrs(input.attrs);
    let mut generated_variants = Vec::new();
    let mut from_impls = Vec::new();
    let mut display_arms = Vec::new();
    let mut constructors = Vec::new();
    let mut used_variant_names = BTreeMap::<String, Span>::new();
    let mut used_constructor_names = BTreeMap::<String, Span>::new();
    let enum_name = &input.name;
    let vis = input.vis;

    for term in input.terms {
        expand_term(
            &mut generated_variants,
            &mut from_impls,
            &mut display_arms,
            &mut constructors,
            &mut used_variant_names,
            &mut used_constructor_names,
            enum_name,
            term,
        )?;
    }

    let merged_attrs = merge_debug_derive(attrs)?;
    Ok(quote! {
        #(#merged_attrs)*
        #vis enum #enum_name {
            #(#generated_variants),*
        }

        impl #enum_name {
            #(#constructors)*
        }

        impl ::core::fmt::Display for #enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#display_arms),*
                }
            }
        }

        impl ::core::error::Error for #enum_name {}

        #(#from_impls)*
    })
}

fn merge_debug_derive(attrs: Vec<Attribute>) -> Result<Vec<Attribute>> {
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

fn strip_union_attrs(attrs: Vec<Attribute>) -> Vec<Attribute> {
    attrs
        .into_iter()
        .filter(|attr| !attr.path().is_ident("union"))
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

fn sanitize_variant(variant: &Variant) -> Variant {
    let mut sanitized = variant.clone();
    sanitized.attrs = sanitize_attrs(&sanitized.attrs);
    sanitized
}

fn sanitize_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("display"))
        .cloned()
        .collect()
}

fn display_arm(enum_ident: &Ident, variant: &Variant) -> Result<proc_macro2::TokenStream> {
    let vn = &variant.ident;
    let template = parse_display_template(&variant.attrs)?;
    match &variant.fields {
        Fields::Unit => display_arm_unit(enum_ident, vn, template),
        Fields::Named(named) => display_arm_named(enum_ident, vn, template, named),
        Fields::Unnamed(unnamed) => display_arm_unnamed(enum_ident, vn, template, unnamed),
    }
}

fn display_arm_unit(
    enum_ident: &Ident,
    vn: &Ident,
    template: Option<LitStr>,
) -> Result<proc_macro2::TokenStream> {
    let expr = display_expr(enum_ident, vn, template.as_ref(), &[])?;
    Ok(quote! { #enum_ident::#vn => { #expr } })
}

fn display_arm_named(
    enum_ident: &Ident,
    vn: &Ident,
    template: Option<LitStr>,
    named: &syn::FieldsNamed,
) -> Result<proc_macro2::TokenStream> {
    let mut idents = Vec::new();
    for field in &named.named {
        let id = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "named field should have ident"))?
            .clone();
        idents.push(id);
    }
    let replacements = idents
        .iter()
        .map(|ident| (ident.to_string(), quote! { #ident }))
        .collect::<Vec<_>>();
    let expr = display_expr(enum_ident, vn, template.as_ref(), &replacements)?;
    Ok(quote! { #enum_ident::#vn { #(#idents),* } => { #expr } })
}

fn display_arm_unnamed(
    enum_ident: &Ident,
    vn: &Ident,
    template: Option<LitStr>,
    unnamed: &syn::FieldsUnnamed,
) -> Result<proc_macro2::TokenStream> {
    let binders = (0..unnamed.unnamed.len())
        .map(|idx| quote::format_ident!("f{idx}"))
        .collect::<Vec<_>>();
    let replacements = binders
        .iter()
        .enumerate()
        .map(|(idx, ident)| (idx.to_string(), quote! { #ident }))
        .collect::<Vec<_>>();
    let expr = display_expr(enum_ident, vn, template.as_ref(), &replacements)?;
    Ok(quote! { #enum_ident::#vn(#(#binders),*) => { #expr } })
}

fn parse_display_template(attrs: &[Attribute]) -> Result<Option<LitStr>> {
    let mut template: Option<LitStr> = None;
    for attr in attrs {
        if !attr.path().is_ident("display") {
            continue;
        }
        let lit = attr.parse_args::<LitStr>()?;
        if template.replace(lit).is_some() {
            return Err(Error::new_spanned(
                attr,
                "duplicate #[display(...)] on variant",
            ));
        }
    }
    Ok(template)
}

fn display_expr(
    enum_ident: &Ident,
    variant_name: &Ident,
    template: Option<&LitStr>,
    replacements: &[(String, proc_macro2::TokenStream)],
) -> Result<proc_macro2::TokenStream> {
    if let Some(template_lit) = template {
        let (fmt_template, ordered_tokens) = render_display_template(template_lit, replacements)?;
        return Ok(quote! {
            write!(f, #fmt_template #(, #ordered_tokens)*)
        });
    }
    Ok(quote! {
        write!(f, "{}::{}", stringify!(#enum_ident), stringify!(#variant_name))
    })
}

fn render_display_template(
    template: &LitStr,
    replacements: &[(String, proc_macro2::TokenStream)],
) -> Result<(String, Vec<proc_macro2::TokenStream>)> {
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

fn expand_term(
    generated_variants: &mut Vec<proc_macro2::TokenStream>,
    from_impls: &mut Vec<proc_macro2::TokenStream>,
    display_arms: &mut Vec<proc_macro2::TokenStream>,
    constructors: &mut Vec<proc_macro2::TokenStream>,
    used_variant_names: &mut BTreeMap<String, Span>,
    used_constructor_names: &mut BTreeMap<String, Span>,
    enum_name: &Ident,
    term: UnionItem,
) -> Result<()> {
    match term {
        UnionItem::External { ty, alias } => expand_external(
            generated_variants,
            from_impls,
            display_arms,
            constructors,
            used_variant_names,
            used_constructor_names,
            enum_name,
            ty,
            alias,
        ),
        UnionItem::Inline(inline) => expand_inline_item(
            generated_variants,
            display_arms,
            constructors,
            used_variant_names,
            used_constructor_names,
            enum_name,
            inline,
        ),
    }
}

fn expand_external(
    generated_variants: &mut Vec<proc_macro2::TokenStream>,
    from_impls: &mut Vec<proc_macro2::TokenStream>,
    display_arms: &mut Vec<proc_macro2::TokenStream>,
    constructors: &mut Vec<proc_macro2::TokenStream>,
    used_variant_names: &mut BTreeMap<String, Span>,
    used_constructor_names: &mut BTreeMap<String, Span>,
    enum_name: &Ident,
    ty: syn::TypePath,
    alias: Option<Ident>,
) -> Result<()> {
    let variant_ident = alias.unwrap_or_else(|| {
        let last = ty.path.segments.last().map(|s| &s.ident);
        match last {
            Some(id) => id.clone(),
            None => Ident::new("Unknown", Span::call_site()),
        }
    });
    check_unique_variant(&variant_ident, used_variant_names, variant_ident.span())?;
    generated_variants.push(quote! { #variant_ident(#ty) });
    display_arms.push(quote! {
        #enum_name::#variant_ident(inner) => write!(f, "{}", inner)
    });
    constructors.push(generate_constructor(
        enum_name,
        &variant_ident,
        &syn::Fields::Unnamed(syn::parse_quote!((#ty))),
        used_constructor_names,
    )?);
    from_impls.push(quote! {
        impl ::core::convert::From<#ty> for #enum_name {
            fn from(value: #ty) -> Self { Self::#variant_ident(value) }
        }
    });
    Ok(())
}

fn expand_inline_item(
    generated_variants: &mut Vec<proc_macro2::TokenStream>,
    display_arms: &mut Vec<proc_macro2::TokenStream>,
    constructors: &mut Vec<proc_macro2::TokenStream>,
    used_variant_names: &mut BTreeMap<String, Span>,
    used_constructor_names: &mut BTreeMap<String, Span>,
    enum_name: &Ident,
    inline: InlineVariants,
) -> Result<()> {
    for variant in inline.variants {
        check_unique_variant(&variant.ident, used_variant_names, variant.ident.span())?;
        display_arms.push(display_arm(enum_name, &variant)?);
        constructors.push(generate_constructor(
            enum_name,
            &variant.ident,
            &variant.fields,
            used_constructor_names,
        )?);
        let variant = sanitize_variant(&variant);
        generated_variants.push(quote! { #variant });
    }
    Ok(())
}

fn generate_constructor(
    enum_name: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
    used_constructor_names: &mut BTreeMap<String, Span>,
) -> Result<proc_macro2::TokenStream> {
    let ctor_name = to_snake_case(&variant_ident.to_string());
    check_unique_constructor_name(
        enum_name,
        &ctor_name,
        variant_ident.span(),
        used_constructor_names,
    )?;
    let ctor_ident = Ident::new(&ctor_name, variant_ident.span());
    let ctor_report_ident = Ident::new(&format!("{ctor_name}_report"), variant_ident.span());
    let (params, fields_gen) = expand_constructor_fields(fields)?;
    Ok(quote! {
        pub fn #ctor_ident(#(#params),*) -> Self {
            Self::#variant_ident #fields_gen
        }
        pub fn #ctor_report_ident(#(#params),*) -> ::diagweave::report::Report<Self> {
            ::diagweave::report::Report::new(Self::#variant_ident #fields_gen)
        }
    })
}

fn check_unique_constructor_name(
    enum_name: &Ident,
    constructor_name: &str,
    span: Span,
    used_constructor_names: &mut BTreeMap<String, Span>,
) -> Result<()> {
    if let Some(_previous) = used_constructor_names.get(constructor_name) {
        return Err(Error::new(
            span,
            format!("constructor name collision `{}` in `{}`", constructor_name, enum_name),
        ));
    }
    used_constructor_names.insert(constructor_name.to_owned(), span);
    Ok(())
}

fn expand_constructor_fields(fields: &Fields) -> Result<(Vec<proc_macro2::TokenStream>, proc_macro2::TokenStream)> {
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
                    let ident = quote::format_ident!("arg{idx}");
                    let ty = &field.ty;
                    quote! { #ident: #ty }
                })
                .collect::<Vec<_>>();
            let idents = (0..fields_unnamed.unnamed.len()).map(|idx| quote::format_ident!("arg{idx}"));
            Ok((params, quote! { (#(#idents),*) }))
        }
    }
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
    replacements: &[(String, proc_macro2::TokenStream)],
    ordered: &mut Vec<proc_macro2::TokenStream>,
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
