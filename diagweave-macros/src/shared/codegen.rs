use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub(crate) fn enum_impl_helpers(enum_ident: &Ident, source_arms: &[TokenStream]) -> TokenStream {
    quote! {
        impl #enum_ident {
            pub fn to_report(self) -> ::diagweave::report::Report<Self> { ::diagweave::report::Report::new(self) }
            /// Convenience: allow direct `.diag(...)` calls on client error types.
            /// This is a generic variant that allows transforming both the error type
            /// and the state type. When only adding metadata, no explicit type
            /// annotations are needed.
            pub fn diag<E2, State2>(
                self,
                f: impl FnOnce(::diagweave::report::Report<Self>) -> ::diagweave::report::Report<E2, State2>,
            ) -> ::diagweave::report::Report<E2, State2>
            where
                State2: ::diagweave::report::SeverityState,
            {
                f(self.to_report())
            }
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
