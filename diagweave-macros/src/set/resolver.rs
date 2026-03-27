use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::quote;
use syn::{Attribute, Error, Ident, Result, Variant};

use crate::set::bitset::{BitSet, SymbolTable};
use crate::set::parser::{InlineVariants, SetDecl, UnionTerm};

#[derive(Clone)]
pub(crate) struct VariantIdentity {
    pub(crate) name: String,
    pub(crate) signature: String,
}

#[derive(Clone)]
pub(crate) struct ResolvedVariant {
    pub(crate) symbol: usize,
    pub(crate) identity: VariantIdentity,
    pub(crate) variant: Variant,
}

#[derive(Clone)]
pub(crate) struct ResolvedSet {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) name: Ident,
    pub(crate) variants: Vec<ResolvedVariant>,
    pub(crate) members: BitSet,
}

pub(crate) fn collect_decls(decls: Vec<SetDecl>) -> Result<BTreeMap<String, SetDecl>> {
    let mut map = BTreeMap::<String, SetDecl>::new();
    for decl in decls {
        let key = decl.name.to_string();
        if let Some(existing) = map.get(&key) {
            return Err(Error::new(
                decl.name.span(),
                format!(
                    "set `{}` is defined more than once; first definition span: {:?}",
                    key,
                    existing.name.span()
                ),
            ));
        }
        map.insert(key, decl);
    }
    Ok(map)
}

pub(crate) fn resolve_set(
    name: &str,
    decls: &BTreeMap<String, SetDecl>,
    resolved: &mut BTreeMap<String, ResolvedSet>,
    stack: &mut Vec<String>,
    symbol_table: &mut SymbolTable,
) -> Result<()> {
    if resolved.contains_key(name) {
        return Ok(());
    }

    if stack.iter().any(|n| n == name) {
        let mut chain = stack.clone();
        chain.push(name.to_owned());
        return Err(Error::new(
            Span::call_site(),
            format!("cyclic dependency detected: {}", chain.join(" -> ")),
        ));
    }

    let decl = decls
        .get(name)
        .ok_or_else(|| Error::new(Span::call_site(), format!("unknown set `{name}`")))?;
    stack.push(name.to_owned());

    let mut variants = Vec::<ResolvedVariant>::new();
    let mut signatures = BTreeMap::<String, VariantIdentity>::new();
    let mut members = BitSet::with_capacity(symbol_table.len());

    {
        let mut ctx = ResolveContext {
            decls,
            resolved,
            stack,
            symbol_table,
            variants: &mut variants,
            signatures: &mut signatures,
            members: &mut members,
        };

        for term in &decl.expr.terms {
            ctx.resolve_term(term)?;
        }
    }

    stack.pop();
    resolved.insert(
        name.to_owned(),
        ResolvedSet {
            attrs: decl.attrs.clone(),
            vis: decl.vis.clone(),
            name: decl.name.clone(),
            variants,
            members,
        },
    );
    Ok(())
}

struct ResolveContext<'a> {
    decls: &'a BTreeMap<String, SetDecl>,
    resolved: &'a mut BTreeMap<String, ResolvedSet>,
    stack: &'a mut Vec<String>,
    symbol_table: &'a mut SymbolTable,
    variants: &'a mut Vec<ResolvedVariant>,
    signatures: &'a mut BTreeMap<String, VariantIdentity>,
    members: &'a mut BitSet,
}

impl<'a> ResolveContext<'a> {
    fn try_push_variant(&mut self, variant: ResolvedVariant) -> Result<()> {
        let name = variant.identity.name.clone();
        match self.signatures.get(&name) {
            Some(existing) if existing.signature == variant.identity.signature => {
                self.members.insert(variant.symbol);
                Ok(())
            }
            Some(_) => Err(Error::new_spanned(
                &variant.variant.ident,
                format!(
                    "variant `{name}` has the same name but a different field shape; cannot deduplicate union members"
                ),
            )),
            None => {
                self.signatures.insert(name, variant.identity.clone());
                self.members.insert(variant.symbol);
                self.variants.push(variant);
                Ok(())
            }
        }
    }

    fn resolve_term(&mut self, term: &UnionTerm) -> Result<()> {
        match term {
            UnionTerm::SetRef(ident) => self.resolve_set_ref(ident),
            UnionTerm::Inline(inline) => self.resolve_inline(inline),
        }
    }

    fn resolve_set_ref(&mut self, ident: &Ident) -> Result<()> {
        let ref_name = ident.to_string();
        if !self.decls.contains_key(&ref_name) {
            return Err(Error::new_spanned(
                ident,
                format!("referenced undefined set `{ref_name}`"),
            ));
        }
        resolve_set(
            &ref_name,
            self.decls,
            self.resolved,
            self.stack,
            self.symbol_table,
        )?;
        let (source_members, source_variants) = {
            let source = self
                .resolved
                .get(&ref_name)
                .ok_or_else(|| Error::new(ident.span(), "source set should be resolved"))?;
            (source.members.clone(), source.variants.clone())
        };
        self.members.union_with(&source_members);
        for variant in source_variants {
            self.try_push_variant(variant)?;
        }
        Ok(())
    }

    fn resolve_inline(&mut self, inline: &InlineVariants) -> Result<()> {
        for variant in &inline.variants {
            let sig = variant_signature(variant);
            let sym = self.symbol_table.intern(sig.clone());
            let res = ResolvedVariant {
                symbol: sym,
                identity: VariantIdentity {
                    name: variant.ident.to_string(),
                    signature: sig,
                },
                variant: variant.clone(),
            };
            self.try_push_variant(res)?;
        }
        Ok(())
    }
}

pub(crate) fn variant_signature(variant: &Variant) -> String {
    let mut shape_only = variant.clone();
    shape_only.attrs = vec![];
    for field in shape_only.fields.iter_mut() {
        field.attrs = vec![];
    }
    quote!(#shape_only).to_string()
}
