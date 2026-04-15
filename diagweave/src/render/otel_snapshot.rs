use crate::adapters::{OtelAttribute, OtelEnvelope, OtelEvent, OtelValue};

pub(crate) fn normalize_otel_envelope(envelope: &mut OtelEnvelope<'_>) {
    for record in &mut envelope.records {
        normalize_otel_event(record);
    }
}

fn normalize_otel_event(event: &mut OtelEvent<'_>) {
    normalize_otel_attributes(&mut event.attributes);
}

fn normalize_otel_attributes(attributes: &mut Vec<OtelAttribute<'_>>) {
    attributes.sort_by(|left, right| left.key.as_ref().cmp(right.key.as_ref()));
    for attribute in attributes.iter_mut() {
        normalize_otel_value(&mut attribute.value);
    }
}

fn normalize_otel_value(value: &mut OtelValue<'_>) {
    match value {
        OtelValue::Array(items) => {
            for item in items {
                normalize_otel_value(item);
            }
        }
        OtelValue::KvList(attributes) => normalize_otel_attributes(attributes),
        _ => {}
    }
}
