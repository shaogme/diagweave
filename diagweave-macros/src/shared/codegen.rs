use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub(crate) fn enum_impl_helpers(enum_ident: &Ident, source_arms: &[TokenStream]) -> TokenStream {
    quote! {
        impl #enum_ident {
            pub fn diag(self) -> ::diagweave::report::Report<Self> { ::diagweave::report::Report::new(self) }
            pub fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                <Self as ::core::error::Error>::source(self)
            }
        }
        impl ::core::error::Error for #enum_ident {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                match self {
                    #(#source_arms),*
                }
            }
        }
    }
}
