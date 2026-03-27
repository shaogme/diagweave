use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Error, Fields, Ident, LitStr, Result, Variant};

#[derive(Clone)]
pub(crate) enum DisplayMode {
    Default,
    Template(LitStr),
    Transparent,
}

pub(crate) fn display_arm(enum_ident: &Ident, variant: &Variant) -> Result<TokenStream> {
    let variant_name = &variant.ident;
    let display = parse_display_mode(&variant.attrs)?;
    match &variant.fields {
        Fields::Unit => display_arm_unit(enum_ident, variant_name, display),
        Fields::Named(named) => display_arm_named(enum_ident, variant_name, display, named),
        Fields::Unnamed(unnamed) => display_arm_unnamed(enum_ident, variant_name, display, unnamed),
    }
}

pub(crate) fn parse_display_mode(attrs: &[Attribute]) -> Result<DisplayMode> {
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
