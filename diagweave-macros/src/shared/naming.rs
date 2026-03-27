pub(crate) fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = input.chars().collect();
    for (idx, ch) in chars.iter().enumerate() {
        let is_upper = ch.is_ascii_uppercase();
        if is_upper {
            if idx > 0 {
                let prev = chars[idx - 1];
                let next_is_lower = chars
                    .get(idx + 1)
                    .map(|next| next.is_ascii_lowercase())
                    .unwrap_or(false);
                if prev.is_ascii_lowercase() || next_is_lower {
                    out.push('_');
                }
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(*ch);
        }
    }
    out
}
