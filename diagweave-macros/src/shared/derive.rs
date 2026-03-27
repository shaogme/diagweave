use std::collections::BTreeMap;

use syn::punctuated::Punctuated;
use syn::{Attribute, Path, Result, Token};

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
