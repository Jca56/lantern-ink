//! Entities: `&amp;` and friends read to the characters they stand for,
//! and those characters written back as entities where XML needs them.

use std::borrow::Cow;

/// The character a reference names: the five XML defines, and numeric
/// ones (`#38`, `#x26`).
fn entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let code = name.strip_prefix('#')?;
            let n = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(n)
        }
    }
}

/// The longest a reference's name is looked for (`#x10FFFF`).
const MAX_ENTITY: usize = 10;

/// `raw` with its entities resolved. A reference that names nothing known
/// (one a DOCTYPE declared, or a stray `&`) stays as written.
pub(crate) fn unescape(raw: &str) -> Cow<'_, str> {
    if !raw.contains('&') {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let named = tail.bytes().take(MAX_ENTITY + 1).position(|b| b == b';').and_then(|semi| entity(&tail[..semi]).map(|c| (c, semi)));
        match named {
            Some((c, semi)) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// `value` as it goes between `quote`s in a tag.
pub(crate) fn escape_attr(value: &str, quote: u8) -> Cow<'_, str> {
    let quote = quote as char;
    if !value.contains(['&', '<', quote]) {
        return Cow::Borrowed(value);
    }
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' if quote == '"' => out.push_str("&quot;"),
            '\'' if quote == '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_resolve_and_unknown_ones_stay() {
        assert_eq!(unescape("a &lt; b &amp;&amp; c &gt; d"), "a < b && c > d");
        assert_eq!(unescape("&quot;x&apos; &#65;&#x42;&#X43;"), "\"x' ABC");
        assert_eq!(unescape("&ns_svg; &; & &#xZZ; &#1114112; &toolongtobeanentity;"), "&ns_svg; &; & &#xZZ; &#1114112; &toolongtobeanentity;");
        assert_eq!(unescape("tail &"), "tail &");
        assert!(matches!(unescape("plain"), Cow::Borrowed(_)));
        assert_eq!(unescape("&#x1F58B;"), "🖋");
    }

    #[test]
    fn values_are_escaped_only_where_they_must_be() {
        assert_eq!(escape_attr("a<b & \"c\" 'd'", b'"'), "a&lt;b &amp; &quot;c&quot; 'd'");
        assert_eq!(escape_attr("a \"c\" 'd'", b'\''), "a \"c\" &apos;d&apos;");
        assert!(matches!(escape_attr("M0 0 L1 1 > 2", b'"'), Cow::Borrowed(_)));
        // What's written reads back as it was.
        for v in ["a<b & \"c\" 'd'", "&amp;", "&#38;"] {
            assert_eq!(unescape(&escape_attr(v, b'"')), v);
        }
    }
}
