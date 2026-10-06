//! Colours as SVG writes them: hex, `rgb()`, `hsl()`, and CSS's names.

use lntrn_math::Color;

use crate::length::Length;

/// CSS's named colours, in order (they're looked up by halving).
const NAMED: [(&str, u32); 148] = [
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// A byte from a number that may be anything.
fn byte(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// The colour `s` names; `None` for what isn't a colour (`none` is a
/// paint, not a colour). `currentColor` is black, as an icon with no
/// context would show it.
pub fn parse(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.is_ascii() {
            return None;
        }
        let two = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let one = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|x| x * 17);
        let (r, g, b, a) = match hex.len() {
            3 => (one(0)?, one(1)?, one(2)?, 255),
            4 => (one(0)?, one(1)?, one(2)?, one(3)?),
            6 => (two(0)?, two(2)?, two(4)?, 255),
            8 => (two(0)?, two(2)?, two(4)?, two(6)?),
            _ => return None,
        };
        return Some(Color::from_u8(r, g, b, a));
    }
    let lower = s.to_ascii_lowercase();
    // What's between the brackets of `name(…)` or `namea(…)`, split at
    // commas, spaces and the slash before an alpha.
    let function = |name: &str| -> Option<Vec<&str>> {
        let rest = lower.strip_prefix(name)?.trim_start_matches('a').trim_start().strip_prefix('(')?;
        Some(rest.trim_end_matches(')').split([',', ' ', '/']).filter(|p| !p.is_empty()).collect())
    };
    // An alpha: a number 0..1 or a percentage; opaque when there's none
    // (or none that reads).
    let alpha = |p: Option<&&str>| p.and_then(|p| Length::parse(p)).map_or(255, |a| byte(a.fraction().clamp(0.0, 1.0) * 255.0));
    if let Some(parts) = function("rgb") {
        let channel = |i: usize| -> Option<u8> {
            match Length::parse(parts.get(i)?)? {
                Length::Px(v) => Some(byte(v)),
                Length::Percent(p) => Some(byte(p * 2.55)),
            }
        };
        return Some(Color::from_u8(channel(0)?, channel(1)?, channel(2)?, alpha(parts.get(3))));
    }
    if let Some(parts) = function("hsl") {
        // The hue is an angle (degrees unless it says), the other two
        // percentages.
        let hue = parts.first()?.trim();
        let (number, per_turn) = [("deg", 360.0), ("grad", 400.0), ("rad", std::f64::consts::TAU), ("turn", 1.0)].iter().find_map(|(unit, per)| Some((hue.strip_suffix(unit)?, *per))).unwrap_or((hue, 360.0));
        let h = (ink_geom::number::parse(number)? / per_turn).rem_euclid(1.0) * 6.0;
        let pct = |i: usize| ink_geom::number::parse(parts.get(i)?.trim_end_matches('%')).map(|v| (v * 0.01).clamp(0.0, 1.0));
        let (sat, light) = (pct(1)?, pct(2)?);
        let chroma = (1.0 - (2.0 * light - 1.0).abs()) * sat;
        let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (chroma, x, 0.0),
            1 => (x, chroma, 0.0),
            2 => (0.0, chroma, x),
            3 => (0.0, x, chroma),
            4 => (x, 0.0, chroma),
            _ => (chroma, 0.0, x),
        };
        let channel = |v: f64| byte((v + light - chroma * 0.5) * 255.0);
        return Some(Color::from_u8(channel(r), channel(g), channel(b), alpha(parts.get(3))));
    }
    match lower.as_str() {
        "transparent" => Some(Color::TRANSPARENT),
        "currentcolor" => Some(Color::BLACK),
        name => NAMED.binary_search_by(|(n, _)| n.cmp(&name)).ok().map(|i| Color::hex(NAMED[i].1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &str) -> Option<[u8; 4]> {
        parse(s).map(Color::to_u8)
    }

    #[test]
    fn reads_hex_and_functions() {
        assert_eq!(bytes("#f80"), Some([255, 136, 0, 255]));
        assert_eq!(bytes("#F808"), Some([255, 136, 0, 136]));
        assert_eq!(bytes(" #12345678 "), Some([0x12, 0x34, 0x56, 0x78]));
        assert_eq!((bytes("#12"), bytes("#12345"), bytes("#ggg"), bytes("#ééé")), (None, None, None, None));
        assert_eq!(bytes("rgb(0% 53.566% 62.803%)"), Some([0, 137, 160, 255]));
        assert_eq!(bytes("rgb(75, 73, 73)"), Some([75, 73, 73, 255]));
        assert_eq!(bytes("RGB(300, -5, 73)"), Some([255, 0, 73, 255]), "out of range is brought in");
        // An alpha is a number or a percentage, after a comma or a slash.
        assert_eq!(bytes("rgba(255, 0, 0, 0.5)"), Some([255, 0, 0, 128]));
        assert_eq!(bytes("rgb(255 0 0 / 25%)"), Some([255, 0, 0, 64]));
        assert_eq!(bytes("rgb(1 2)"), None);
    }

    #[test]
    fn reads_hue_saturation_and_lightness() {
        assert_eq!(bytes("hsl(14.8, 63.1%, 59.6%)"), Some([217, 119, 87, 255]));
        for (hsl, rgb) in [("0, 100%, 50%", [255, 0, 0]), ("60 100% 50%", [255, 255, 0]), ("120deg 100% 25%", [0, 128, 0]), ("180, 100%, 50%", [0, 255, 255]), ("240 100% 50%", [0, 0, 255]), ("300, 100%, 50%", [255, 0, 255])] {
            assert_eq!(bytes(&format!("hsl({hsl})")), Some([rgb[0], rgb[1], rgb[2], 255]), "{hsl}");
        }
        assert_eq!(bytes("hsl(0.5turn 100% 50%)"), Some([0, 255, 255, 255]));
        assert_eq!(bytes("hsl(-120 100% 50%)"), Some([0, 0, 255, 255]), "a hue wraps");
        assert_eq!(bytes("hsl(200grad 100% 50%)"), bytes("hsl(3.14159265rad 100% 50%)"));
        assert_eq!(bytes("hsl(0 0% 50%)"), Some([128, 128, 128, 255]));
        assert_eq!(bytes("hsla(0, 100%, 50%, 0.5)"), Some([255, 0, 0, 128]));
        assert_eq!(bytes("hsl(0 100% 50% / 50%)"), Some([255, 0, 0, 128]));
        assert_eq!((bytes("hsl(red 1% 1%)"), bytes("hsl(10)"), bytes("hsl()")), (None, None, None));
        // Numbers that aren't: no panic, and no colour.
        for odd in ["hsl(nan 50% 50%)", "hsl(inf 50% 50%)", "hsl(10 nan% inf%)", "hsl(1e999 1e39% 1e39%)", "rgb(nan nan nan / nan)"] {
            assert_eq!(bytes(odd), None, "{odd}");
        }
    }

    #[test]
    fn reads_names() {
        assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0), "the table is in order");
        assert_eq!(bytes("red"), Some([255, 0, 0, 255]));
        assert_eq!(bytes("Green"), Some([0, 128, 0, 255]));
        assert_eq!(bytes("rebeccapurple"), Some([0x66, 0x33, 0x99, 255]));
        assert_eq!(bytes("yellowgreen"), Some([0x9a, 0xcd, 0x32, 255]));
        assert_eq!(bytes("aliceblue"), Some([0xf0, 0xf8, 0xff, 255]));
        assert_eq!(bytes("grey"), bytes("gray"));
        assert_eq!(bytes("transparent"), Some([0, 0, 0, 0]));
        assert_eq!(bytes("currentColor"), Some([0, 0, 0, 255]));
        assert_eq!((bytes("none"), bytes("blurple"), bytes("")), (None, None, None));
    }
}
