//! Lengths as attributes write them: a number, with a unit or a percent
//! sign or neither.

use ink_geom::number::scan;

/// User units in an inch (CSS's reference pixel).
const PER_INCH: f64 = 96.0;
/// The font size `em` is taken against where nothing says a font: what
/// `font-size: medium` comes to.
pub const EM: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// In user units: a plain number, `px`, or an absolute unit turned
    /// into them.
    Px(f64),
    Percent(f64),
}

impl Length {
    /// `12`, `12px`, `1.5pt`, `50%`. `None` for anything else.
    pub fn parse(s: &str) -> Option<Length> {
        Length::parse_in(s, EM)
    }

    /// As [`Length::parse`], where the font's size is `em`: what text
    /// measures `em` and `ex` against.
    pub fn parse_in(s: &str, em: f64) -> Option<Length> {
        let bytes = s.trim().as_bytes();
        let mut i = 0;
        let v = scan(bytes, &mut i)?;
        let unit = std::str::from_utf8(&bytes[i..]).ok()?;
        let per = match unit.to_ascii_lowercase().as_str() {
            "%" => return Some(Length::Percent(v)),
            "" | "px" => 1.0,
            "pt" => PER_INCH / 72.0,
            "pc" => PER_INCH / 6.0,
            "in" => PER_INCH,
            "cm" => PER_INCH / 2.54,
            "mm" => PER_INCH / 25.4,
            "q" => PER_INCH / 101.6,
            "em" => em,
            "ex" => em / 2.0,
            _ => return None,
        };
        Some(Length::Px(v * per)).filter(|_| (v * per).is_finite())
    }

    /// In user units, a percentage being of `whole`.
    pub fn of(self, whole: f64) -> f64 {
        match self {
            Length::Px(v) => v,
            Length::Percent(p) => p * 0.01 * whole,
        }
    }

    /// As a fraction of a box: `50%` and `0.5` are both a half.
    pub fn fraction(self) -> f64 {
        match self {
            Length::Px(v) => v,
            Length::Percent(p) => p * 0.01,
        }
    }
}

/// A length in user units. A percentage isn't one (there's nothing here
/// to take it of).
pub fn number(s: &str) -> Option<f64> {
    match Length::parse(s)? {
        Length::Px(v) => Some(v),
        Length::Percent(_) => None,
    }
}

/// A ratio 0..1, as a number or a percentage: an opacity.
pub fn unit(s: &str) -> Option<f64> {
    Length::parse(s).map(|l| l.fraction().clamp(0.0, 1.0))
}

/// The lengths in a list split by spaces or commas, in user units;
/// `None` if anything in it isn't one.
pub fn numbers(s: &str) -> Option<Vec<f64>> {
    s.split(|c: char| c.is_whitespace() || c == ',').filter(|p| !p.is_empty()).map(number).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lengths_in_their_units() {
        assert_eq!(Length::parse("50%"), Some(Length::Percent(50.0)));
        assert_eq!(Length::parse(" 12px "), Some(Length::Px(12.0)));
        assert_eq!(Length::parse("12PX"), Some(Length::Px(12.0)));
        assert_eq!(Length::parse("-.5"), Some(Length::Px(-0.5)));
        assert_eq!(number("72pt"), Some(96.0));
        assert_eq!(number("1in"), Some(96.0));
        assert_eq!(number("25.4mm"), Some(96.0));
        assert_eq!(number("2em"), Some(32.0));
        assert_eq!((Length::parse_in("1.5em", 10.0), Length::parse_in("1ex", 10.0), Length::parse_in("3", 10.0)), (Some(Length::Px(15.0)), Some(Length::Px(5.0)), Some(Length::Px(3.0))));
        for bad in ["", "px", "12 px", "12furlongs", "nan", "inf", "1e999", "12%%"] {
            assert_eq!(Length::parse(bad), None, "{bad:?}");
        }
        assert_eq!(number("50%"), None);
        assert_eq!(Length::parse("25%").map(|l| l.of(200.0)), Some(50.0));
        assert_eq!(Length::parse("12").map(|l| l.of(200.0)), Some(12.0));
    }

    #[test]
    fn reads_ratios_and_lists() {
        assert_eq!((unit("0.5"), unit("50%"), unit("3"), unit("-1")), (Some(0.5), Some(0.5), Some(1.0), Some(0.0)));
        assert_eq!(unit("half"), None);
        assert_eq!(numbers("16, 22"), Some(vec![16.0, 22.0]));
        assert_eq!(numbers("4 2pt"), Some(vec![4.0, 2.0 * 96.0 / 72.0]));
        assert_eq!(numbers("none"), None);
        assert_eq!(numbers(""), Some(vec![]));
    }
}
