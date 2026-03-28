mod bitset;
mod r#gen;
mod parser;
mod resolver;

use std::collections::BTreeMap;

use crate::shared::options::parse_diagweave_options;
use bitset::SymbolTable;
use r#gen::{generate_all_from_impls, generate_enum_impl};
use parser::SetInput;
use proc_macro::TokenStream;
use quote::quote;
use resolver::{ResolvedSet, collect_decls, resolve_set};
use syn::{Result, parse_macro_input};

pub(crate) fn set_impl(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as SetInput);
    match expand(parsed) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(input: SetInput) -> Result<proc_macro2::TokenStream> {
    let options = parse_diagweave_options(&input.attrs)?;
    let decls = collect_decls(input.decls)?;

    let mut symbol_table = SymbolTable::default();
    let mut resolved = BTreeMap::<String, ResolvedSet>::new();
    let names: Vec<String> = decls.keys().cloned().collect();
    for name in &names {
        let mut stack = Vec::<String>::new();
        resolve_set(name, &decls, &mut resolved, &mut stack, &mut symbol_table)?;
    }

    let mut enums = Vec::new();
    for name in &names {
        let set = resolved.get(name).ok_or_else(|| {
            syn::Error::new(proc_macro2::Span::call_site(), "resolved set must exist")
        })?;
        enums.push(generate_enum_impl(set, &options)?);
    }

    let from_impls = generate_all_from_impls(&names, &resolved)?;

    Ok(quote! {
        #(#enums)*
        #(#from_impls)*
    })
}
