//! Numbers as SVG writes them: read out of path data and attribute
//! lists, and written back as short as they can be.

/// Skip the whitespace and commas that separate numbers.
pub fn skip_separators(s: &[u8], i: &mut usize) {
    while *i < s.len() && (s[*i].is_ascii_whitespace() || s[*i] == b',') {
        *i += 1;
    }
}

/// The number at `*i` (after any separators), with the odd shapes
/// `-.5.5` and `1e-3` take; `*i` is left just past it. `None`, and `*i`
/// where it was, when there's no number there or it's past f64's range.
pub fn scan(s: &[u8], i: &mut usize) -> Option<f64> {
    let before = *i;
    skip_separators(s, i);
    let start = *i;
    if *i < s.len() && (s[*i] == b'-' || s[*i] == b'+') {
        *i += 1;
    }
    let (mut seen_dot, mut digits) = (false, 0);
    while *i < s.len() {
        match s[*i] {
            b'0'..=b'9' => digits += 1,
            b'.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        *i += 1;
    }
    if digits == 0 {
        *i = before;
        return None;
    }
    // An exponent only when digits follow it: "1em" is 1, then "em".
    if *i < s.len() && (s[*i] == b'e' || s[*i] == b'E') {
        let mut j = *i + 1;
        if j < s.len() && (s[j] == b'-' || s[j] == b'+') {
            j += 1;
        }
        let first = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > first {
            *i = j;
        }
    }
    let value = std::str::from_utf8(&s[start..*i]).ok().and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite());
    if value.is_none() {
        *i = before;
    }
    value
}

/// The whole of `text` as one number, spaces around it allowed.
pub fn parse(text: &str) -> Option<f64> {
    let (s, mut i) = (text.trim().as_bytes(), 0);
    let v = scan(s, &mut i)?;
    (i == s.len()).then_some(v)
}

/// Every number in `text`, separated by spaces or commas; `None` when
/// anything else is in it.
pub fn parse_list(text: &str) -> Option<Vec<f64>> {
    let (s, mut i, mut out) = (text.as_bytes(), 0, Vec::new());
    loop {
        skip_separators(s, &mut i);
        if i >= s.len() {
            return Some(out);
        }
        out.push(scan(s, &mut i)?);
    }
}

/// The most decimals a number is written with.
pub const MAX_DECIMALS: usize = 12;

/// `v` rounded to `decimals` places and written without what says
/// nothing: no trailing zeros, no `-0`, never an exponent.
pub fn format(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    let mut s = format!("{:.*}", decimals.min(MAX_DECIMALS), v);
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    if s.trim_start_matches('-').bytes().all(|b| b == b'0') { "0".to_owned() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(text: &str) -> Vec<f64> {
        let (s, mut i, mut out) = (text.as_bytes(), 0, Vec::new());
        while let Some(v) = scan(s, &mut i) {
            out.push(v);
        }
        out
    }

    #[test]
    fn reads_numbers_run_together() {
        assert_eq!(all("1-.5.5.5"), vec![1.0, -0.5, 0.5, 0.5]);
        assert_eq!(all("10,20 , 30\n40"), vec![10.0, 20.0, 30.0, 40.0]);
        assert_eq!(all("+3 1e2 1E-2 2.5e+1 4."), vec![3.0, 100.0, 0.01, 25.0, 4.0]);
        assert_eq!(all("1em"), vec![1.0], "no digits after the e: it isn't an exponent");
        assert_eq!(all("- 5"), Vec::<f64>::new(), "a sign alone is no number");
        assert_eq!(all("."), Vec::<f64>::new());
    }

    #[test]
    fn a_failed_read_leaves_the_place_alone() {
        let (s, mut i) = (b"  ,x".as_slice(), 0);
        assert_eq!(scan(s, &mut i), None);
        assert_eq!(i, 0);
        let (s, mut i) = (b"7 1e999".as_slice(), 0);
        assert_eq!(scan(s, &mut i), Some(7.0));
        let at = i;
        assert_eq!(scan(s, &mut i), None, "past f64's range");
        assert_eq!(i, at);
    }

    #[test]
    fn parses_whole_values_and_lists() {
        assert_eq!(parse(" 12.5 "), Some(12.5));
        assert_eq!(parse("12px"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse_list("0 0 24,24"), Some(vec![0.0, 0.0, 24.0, 24.0]));
        assert_eq!(parse_list("  "), Some(vec![]));
        assert_eq!(parse_list("1 two"), None);
    }

    #[test]
    fn writes_them_short() {
        assert_eq!(format(12.0, 3), "12");
        assert_eq!(format(12.5, 3), "12.5");
        assert_eq!(format(0.1 + 0.2, 3), "0.3");
        assert_eq!(format(1.0006, 3), "1.001");
        assert_eq!(format(-0.0004, 3), "0", "never -0");
        assert_eq!(format(-0.25, 3), "-0.25");
        assert_eq!(format(1e21, 3), "1000000000000000000000", "never an exponent");
        assert_eq!(format(2.0 / 3.0, 0), "1");
        assert_eq!(format(f64::NAN, 3), "0");
        assert_eq!(format(100.0, 3), "100", "zeros before the point stay");
    }
}
