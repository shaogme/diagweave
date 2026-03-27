use syn::{
    Attribute, Ident, Result, Token, Variant, Visibility, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::shared::options::{DiagweaveOptions, parse_diagweave_options};

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

pub(crate) type SetOptions = DiagweaveOptions;

pub(crate) fn parse_set_options(attrs: &[Attribute]) -> Result<SetOptions> {
    parse_diagweave_options(attrs)
}

#[derive(Clone)]
pub(crate) struct SetDecl {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: Visibility,
    pub(crate) name: Ident,
    pub(crate) expr: UnionExpr,
}

impl Parse for SetDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse::<Visibility>()?;
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let expr = input.parse::<UnionExpr>()?;
        Ok(Self {
            attrs,
            vis,
            name,
            expr,
        })
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
