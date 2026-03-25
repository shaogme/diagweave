use syn::{
    Attribute, Error, Ident, LitStr, Path, Result, Token, Variant, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub(crate) struct SetInput {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) decls: Vec<SetDecl>,
}

impl Parse for SetInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut attrs = Vec::new();
        while input.peek(Token![#]) {
            let fork = input.fork();
            let attr_vec = fork.call(Attribute::parse_outer)?;
            if let Some(first) = attr_vec.first() {
                if first.path().is_ident("diagweave") {
                    attrs.extend(input.call(Attribute::parse_outer)?);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let mut decls = Vec::new();
        while !input.is_empty() {
            let decl = input.parse::<SetDecl>()?;
            decls.push(decl);
        }
        Ok(Self { attrs, decls })
    }
}

pub(crate) struct SetOptions {
    pub(crate) report_path: Path,
    pub(crate) constructor_prefix: String,
}

impl Default for SetOptions {
    fn default() -> Self {
        Self {
            report_path: syn::parse_quote!(::diagweave::report::Report),
            constructor_prefix: String::new(),
        }
    }
}

pub(crate) fn parse_set_options(attrs: &[Attribute]) -> Result<SetOptions> {
    let mut options = SetOptions::default();
    for attr in attrs {
        if !attr.path().is_ident("diagweave") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("report_path") {
                let value = meta.value()?.parse::<LitStr>()?;
                options.report_path = syn::parse_str::<Path>(&value.value()).map_err(|_| {
                    Error::new_spanned(
                        &value,
                        "invalid report_path; expected a valid Rust type path string",
                    )
                })?;
                return Ok(());
            }
            if meta.path.is_ident("constructor_prefix") {
                let value = meta.value()?.parse::<LitStr>()?;
                let prefix = value.value();
                if !prefix.is_empty()
                    && !prefix
                        .chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                {
                    return Err(Error::new_spanned(
                        &value,
                        "invalid constructor_prefix; expected snake_case identifier fragment",
                    ));
                }
                options.constructor_prefix = prefix;
                return Ok(());
            }
            Err(Error::new_spanned(
                meta.path,
                "unknown diagweave option; supported options: report_path = \"path::to::Report\", constructor_prefix = \"prefix\"",
            ))
        })?;
    }
    Ok(options)
}

#[derive(Clone)]
pub(crate) struct SetDecl {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) name: Ident,
    pub(crate) expr: UnionExpr,
}

impl Parse for SetDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let expr = input.parse::<UnionExpr>()?;
        Ok(Self { attrs, name, expr })
    }
}

#[derive(Clone)]
pub(crate) struct UnionExpr {
    pub(crate) terms: Vec<UnionTerm>,
}

impl Parse for UnionExpr {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let terms = Punctuated::<UnionTerm, Token![|]>::parse_separated_nonempty(input)?;
        Ok(Self {
            terms: terms.into_iter().collect(),
        })
    }
}

#[derive(Clone)]
pub(crate) enum UnionTerm {
    SetRef(Ident),
    Inline(InlineVariants),
}

impl Parse for UnionTerm {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(Ident) {
            return Ok(Self::SetRef(input.parse::<Ident>()?));
        }
        if input.peek(syn::token::Brace) {
            return Ok(Self::Inline(input.parse::<InlineVariants>()?));
        }
        Err(input.error("union term must be a set identifier or an inline variant block"))
    }
}

#[derive(Clone)]
pub(crate) struct InlineVariants {
    pub(crate) variants: Vec<Variant>,
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
