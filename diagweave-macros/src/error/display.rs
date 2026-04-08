use quote::quote;
use syn::{Attribute, Error, Fields, Ident, LitStr, Result, spanned::Spanned};

#[derive(Clone)]
pub(crate) enum ErrorDisplay {
    Template(LitStr),
    Transparent,
}

pub(crate) fn parse_error_display(
    attrs: &[Attribute],
    span: proc_macro2::Span,
) -> Result<ErrorDisplay> {
    let mut parsed: Option<ErrorDisplay> = None;
    for attr in attrs {
        if !attr.path().is_ident("display") {
            continue;
        }
        let current = if let Ok(lit) = attr.parse_args::<LitStr>() {
            ErrorDisplay::Template(lit)
        } else {
            let ident = attr.parse_args::<Ident>()?;
            if ident == "transparent" {
                ErrorDisplay::Transparent
            } else {
                return Err(Error::new_spanned(
                    ident,
                    "unsupported #[display(...)] argument; expected string literal or `transparent`",
                ));
            }
        };
        if parsed.replace(current).is_some() {
            return Err(Error::new_spanned(
                attr,
                "duplicate #[display(...)] attribute",
            ));
        }
    }
    parsed.ok_or_else(|| {
        Error::new(
            span,
            "missing #[display(...)] attribute; expected #[display(\"...\")] or #[display(transparent)]",
        )
    })
}

pub(crate) fn display_expr(
    display: &ErrorDisplay,
    variant_ident: &Ident,
    replacements: &[(String, proc_macro2::TokenStream)],
) -> Result<proc_macro2::TokenStream> {
    match display {
        ErrorDisplay::Template(template) => {
            let (fmt_template, ordered) = render_display_template(template, replacements)?;
            Ok(quote! {
                write!(f, #fmt_template #(, #ordered)*)
            })
        }
        ErrorDisplay::Transparent => {
            if replacements.len() != 1 {
                return Err(Error::new(
                    variant_ident.span(),
                    "#[display(transparent)] requires exactly one field",
                ));
            }
            let inner = &replacements[0].1;
            Ok(quote! {
                write!(f, "{}", #inner)
            })
        }
    }
}

pub(crate) fn replacements(
    fields: &Fields,
    binding: &super::codegen::BindingStyle,
) -> Result<Vec<(String, proc_macro2::TokenStream)>> {
    use super::codegen::BindingStyle;
    match (fields, binding) {
        (Fields::Unit, BindingStyle::Unit) => Ok(Vec::new()),
        (Fields::Named(_), BindingStyle::Named(idents)) => Ok(idents
            .iter()
            .map(|ident| (ident.to_string(), quote! { #ident }))
            .collect()),
        (Fields::Unnamed(_), BindingStyle::Unnamed(idents)) => Ok(idents
            .iter()
            .enumerate()
            .map(|(idx, ident)| (idx.to_string(), quote! { #ident }))
            .collect()),
        _ => Err(Error::new(fields.span(), "internal binding mismatch")),
    }
}

pub(crate) fn render_display_template(
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
