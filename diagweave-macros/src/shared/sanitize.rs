use syn::{Attribute, Variant};

pub(crate) fn sanitize_variant_attrs(variant: &Variant) -> Variant {
    let mut sanitized = variant.clone();
    sanitized.attrs = sanitize_attrs(&sanitized.attrs);
    for field in sanitized.fields.iter_mut() {
        field.attrs = sanitize_attrs(&field.attrs);
    }
    sanitized
}

fn sanitize_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("display") && !attr.path().is_ident("from"))
        .cloned()
        .collect()
}
