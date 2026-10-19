//! Numbers typed with a decimal comma.
//!
//! Input files always use a decimal point, but people in locales that write
//! "0,5" type numbers that way. Everything typed in the user interface goes
//! through [`parse`] or [`normalize`], so both forms are accepted, and the
//! network only ever stores the decimal-point form.

/// The text with a decimal point when it is a number written with a decimal
/// comma ("0,5" → "0.5", "1.234,5" → "1234.5", "1,234.5" → "1234.5");
/// any other text is returned trimmed and otherwise unchanged.
pub fn normalize(s: &str) -> String {
    let t = s.trim();
    if !t.contains(',') || t.parse::<f64>().is_ok() {
        return t.to_string();
    }
    let candidate = match (t.rfind(','), t.rfind('.')) {
        // "1.234,5": the comma is the decimal separator, points group digits
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        // "1,234.5": commas group digits
        (Some(_), Some(_)) => t.replace(',', ""),
        _ if t.matches(',').count() == 1 => t.replace(',', "."),
        _ => return t.to_string(),
    };
    if candidate.parse::<f64>().is_ok() {
        candidate
    } else {
        t.to_string()
    }
}

/// A typed number, with a decimal point or a decimal comma.
pub fn parse(s: &str) -> Option<f64> {
    normalize(s).parse().ok()
}

/// Whether the text is a number (with a decimal point or a decimal comma).
pub fn is_number(s: &str) -> bool {
    parse(s).is_some()
}

/// A number's text with its decimal point written as a comma, for display;
/// any other text is returned unchanged.
pub fn with_comma(s: &str) -> String {
    if s.contains('.') && s.trim().parse::<f64>().is_ok() {
        s.replace('.', ",")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_decimal_comma() {
        assert_eq!(parse("0,5"), Some(0.5));
        assert_eq!(parse(" 12.75 "), Some(12.75));
        assert_eq!(parse("-1,25e-3"), Some(-0.00125));
        assert_eq!(parse("1.234,5"), Some(1234.5));
        assert_eq!(parse("1,234.5"), Some(1234.5));
        assert_eq!(parse("1,2,3"), None);
        assert_eq!(parse("abc"), None);
        assert_eq!(normalize("0,5"), "0.5");
        assert_eq!(normalize("24:00"), "24:00");
        assert_eq!(normalize("a,b"), "a,b");
    }

    #[test]
    fn shows_a_decimal_comma() {
        assert_eq!(with_comma("0.5"), "0,5");
        assert_eq!(with_comma("130"), "130");
        assert_eq!(with_comma("Net1.inp"), "Net1.inp");
        assert_eq!(normalize(&with_comma("-3.25")), "-3.25");
    }
}
