use alloc::str;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::ErrorCode;

const INDENT_SPACES: &str = {
    const LEN: usize = 64;
    const SPACES: [u8; LEN] = [b' '; LEN];
    match str::from_utf8(&SPACES) {
        Ok(s) => s,
        Err(_) => panic!("Invalid UTF-8"),
    }
};
const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

pub(crate) fn write_error_code(f: &mut Formatter<'_>, code: &ErrorCode) -> fmt::Result {
    match code {
        ErrorCode::Integer(v) => write!(f, "{v}"),
        ErrorCode::String(v) => write_json_string(f, v),
    }
}

pub(crate) fn write_json_display(
    f: &mut Formatter<'_>,
    value: &(impl Display + ?Sized),
) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        write!(&mut escaper, "{value}")?;
    }
    f.write_char('"')
}

pub(crate) fn write_json_string(f: &mut Formatter<'_>, value: impl AsRef<str>) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        escaper.write_str(value.as_ref())?;
    }
    f.write_char('"')
}

pub(crate) fn write_object_field<F>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    key: &str,
    mut write_value: F,
) -> fmt::Result
where
    F: FnMut(&mut Formatter<'_>) -> fmt::Result,
{
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    write_json_string(f, key)?;
    f.write_char(':')?;
    if pretty {
        f.write_char(' ')?;
    }
    write_value(f)
}

pub(crate) fn write_array_item_prefix(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
) -> fmt::Result {
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    Ok(())
}

pub(crate) fn close_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    empty: bool,
) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char('}')
}

pub(crate) fn close_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    empty: bool,
) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char(']')
}

pub(crate) fn write_indent(f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
    let mut remaining = depth.saturating_mul(2);
    while remaining > 0 {
        let chunk = remaining.min(INDENT_SPACES.len());
        f.write_str(&INDENT_SPACES[..chunk])?;
        remaining -= chunk;
    }
    Ok(())
}

struct JsonStringEscaper<'a, 'b> {
    out: &'a mut Formatter<'b>,
}

impl Write for JsonStringEscaper<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let mut start = 0usize;

        for (idx, &b) in bytes.iter().enumerate() {
            let escaped = match b {
                b'"' => Some("\\\""),
                b'\\' => Some("\\\\"),
                b'\n' => Some("\\n"),
                b'\r' => Some("\\r"),
                b'\t' => Some("\\t"),
                0x08 => Some("\\b"),
                0x0C => Some("\\f"),
                _ => None,
            };

            if let Some(seq) = escaped {
                if start < idx {
                    self.out.write_str(&s[start..idx])?;
                }
                self.out.write_str(seq)?;
                start = idx + 1;
                continue;
            }

            if b <= 0x1F {
                if start < idx {
                    self.out.write_str(&s[start..idx])?;
                }
                self.out.write_str("\\u00")?;
                self.out.write_char(HEX_DIGITS[(b >> 4) as usize] as char)?;
                self.out
                    .write_char(HEX_DIGITS[(b & 0x0F) as usize] as char)?;
                start = idx + 1;
            }
        }

        if start < s.len() {
            self.out.write_str(&s[start..])?;
        }

        Ok(())
    }
}
