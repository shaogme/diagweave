use syn::{Attribute, Error, LitStr, Path, Result};

pub(crate) struct DiagweaveOptions {
    pub(crate) report_path: Path,
    pub(crate) constructor_prefix: String,
}

impl Default for DiagweaveOptions {
    fn default() -> Self {
        Self {
            report_path: syn::parse_quote!(::diagweave::report::Report),
            constructor_prefix: String::new(),
        }
    }
}

pub(crate) fn parse_diagweave_options(attrs: &[Attribute]) -> Result<DiagweaveOptions> {
    let mut options = DiagweaveOptions::default();
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
