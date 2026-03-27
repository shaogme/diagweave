mod error;
mod set;
mod shared;
mod union;

use proc_macro::TokenStream;

#[proc_macro]
pub fn set(input: TokenStream) -> TokenStream {
    set::set_impl(input)
}

#[proc_macro]
pub fn union(input: TokenStream) -> TokenStream {
    union::union_impl(input)
}

#[proc_macro_derive(Error, attributes(display, from, source))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    error::derive_error_impl(input)
}
