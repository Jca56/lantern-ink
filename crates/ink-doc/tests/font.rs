//! The fonts the tests set text in: `tests/fonts/InkTest-Regular.ttf`
//! and `InkTest-Bold.ttf`, family "Ink Test". Made here, so that every
//! number in them is known and every machine sets test text the same:
//! 1000 units to the em, 800 above the baseline and 200 below, no
//! kerning, and glyphs that are boxes (and one ring, for curves).
//!
//! | Characters | Glyph (units) | Advance |
//! |---|---|---|
//! | `A`–`Z` but `I` and `O` | a box 100..500 across, 0..700 up | 600 |
//! | `a`–`z` but those below | a box 100..500 across, 0..500 up | 600 |
//! | `g` `j` `p` `q` `y` | a box 100..500 across, -200..500 up | 600 |
//! | `I` `i` `l` | a box 100..200 across, 0..700 up | 300 |
//! | `O` `o` `0` | a ring 50..550 across, 0..700 up, of curves | 600 |
//! | `1`–`9` | a box 100..450 across, 0..700 up | 550 |
//! | `.` | a box 100..200 across, 0..100 up | 300 |
//! | `-` | a box 100..400 across, 250..350 up | 500 |
//! | space, no-break space | nothing | 300 |
//!
//! The bold face's boxes are 50 wider on each side; its advances are the
//! same.
//!
//! The test here holds the files to what this makes. After changing it:
//! `INK_BLESS=1 cargo test -p ink-doc --test font`.

use std::path::PathBuf;

fn u16s(out: &mut Vec<u8>, vals: &[u16]) {
    for v in vals {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn u32s(out: &mut Vec<u8>, vals: &[u32]) {
    for v in vals {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

/// A point of a contour: on the outline, or a curve's control point.
type Point = (i16, i16, bool);

/// A `glyf` record of `contours`.
fn glyph(contours: &[Vec<Point>]) -> Vec<u8> {
    let points: Vec<Point> = contours.iter().flatten().copied().collect();
    if points.is_empty() {
        return Vec::new();
    }
    let (xs, ys) = (points.iter().map(|p| p.0), points.iter().map(|p| p.1));
    let mut g = Vec::new();
    u16s(&mut g, &[contours.len() as u16, xs.clone().min().unwrap() as u16, ys.clone().min().unwrap() as u16, xs.max().unwrap() as u16, ys.max().unwrap() as u16]);
    let mut total = 0;
    for contour in contours {
        total += contour.len();
        u16s(&mut g, &[total as u16 - 1]);
    }
    u16s(&mut g, &[0]);
    g.extend(points.iter().map(|p| u8::from(p.2)));
    let (mut x, mut y) = (0i16, 0i16);
    for p in &points {
        u16s(&mut g, &[p.0.wrapping_sub(x) as u16]);
        x = p.0;
    }
    for p in &points {
        u16s(&mut g, &[p.1.wrapping_sub(y) as u16]);
        y = p.1;
    }
    while g.len() % 4 != 0 {
        g.push(0);
    }
    g
}

/// A box, clockwise as TrueType draws an outside.
fn boxed(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<Vec<Point>> {
    vec![vec![(x0, y0, true), (x0, y1, true), (x1, y1, true), (x1, y0, true)]]
}

/// A ring: an oval of four curves, and a smaller one the other way
/// round inside it.
fn ring(grow: i16) -> Vec<Vec<Point>> {
    let (l, r, b, t, cx, cy) = (50 - grow, 550 + grow, 0, 700, 300, 350);
    let outside = vec![(cx, t, true), (r, t, false), (r, cy, true), (r, b, false), (cx, b, true), (l, b, false), (l, cy, true), (l, t, false)];
    let (l, r, b, t) = (150, 450, 150, 550);
    let inside = vec![(cx, t, true), (l, t, false), (l, cy, true), (l, b, false), (cx, b, true), (r, b, false), (r, cy, true), (r, t, false)];
    vec![outside, inside]
}

/// A name table of one family and one style.
fn names(style: &str) -> Vec<u8> {
    let said = [(1u16, "Ink Test"), (2, style)];
    let strings: Vec<Vec<u8>> = said.iter().map(|(_, s)| s.encode_utf16().flat_map(u16::to_be_bytes).collect()).collect();
    let mut t = Vec::new();
    u16s(&mut t, &[0, said.len() as u16, 6 + said.len() as u16 * 12]);
    let mut at = 0;
    for ((id, _), s) in said.iter().zip(&strings) {
        u16s(&mut t, &[3, 1, 0x409, *id, s.len() as u16, at]);
        at += s.len() as u16;
    }
    t.extend(strings.concat());
    t
}

/// The font: `grow` units more on each side of every box.
fn font(bold: bool) -> Vec<u8> {
    let grow = if bold { 50 } else { 0 };
    let wide = |x0: i16, y0: i16, x1: i16, y1: i16| boxed(x0 - grow, y0, x1 + grow, y1);
    // Each glyph, its advance, and the characters it stands for.
    let glyphs: Vec<(Vec<Vec<Point>>, u16, String)> = vec![
        (wide(100, 0, 400, 700), 500, String::new()),
        (Vec::new(), 300, " \u{a0}".into()),
        (wide(100, 0, 500, 700), 600, ('A'..='Z').filter(|c| !"IO".contains(*c)).collect()),
        (wide(100, 0, 500, 500), 600, ('a'..='z').filter(|c| !"gjpqyilo".contains(*c)).collect()),
        (wide(100, -200, 500, 500), 600, "gjpqy".into()),
        (wide(100, 0, 200, 700), 300, "Iil".into()),
        (ring(grow), 600, "Oo0".into()),
        (wide(100, 0, 450, 700), 550, ('1'..='9').collect()),
        (wide(100, 0, 200, 100), 300, ".".into()),
        (wide(100, 250, 400, 350), 500, "-".into()),
    ];
    let n = glyphs.len() as u16;

    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[44..46].copy_from_slice(&u16::from(bold).to_be_bytes());
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[34..36].copy_from_slice(&n.to_be_bytes());
    let mut maxp = Vec::new();
    u32s(&mut maxp, &[0x0000_5000]);
    u16s(&mut maxp, &[n]);
    let mut os2 = vec![0u8; 96];
    os2[4..6].copy_from_slice(&(if bold { 700u16 } else { 400 }).to_be_bytes());
    os2[6..8].copy_from_slice(&5u16.to_be_bytes());
    os2[62..64].copy_from_slice(&(if bold { 0x20u16 } else { 0x40 }).to_be_bytes());

    let (mut hmtx, mut loca, mut glyf) = (Vec::new(), Vec::new(), Vec::new());
    let mut mapped: Vec<(u32, u32)> = Vec::new();
    for (gid, (contours, advance, chars)) in glyphs.iter().enumerate() {
        u16s(&mut hmtx, &[*advance, contours.iter().flatten().map(|p| p.0).min().unwrap_or(0) as u16]);
        u32s(&mut loca, &[glyf.len() as u32]);
        glyf.extend(glyph(contours));
        mapped.extend(chars.chars().map(|c| (c as u32, gid as u32)));
    }
    u32s(&mut loca, &[glyf.len() as u32]);
    // One group a character, in the characters' order.
    mapped.sort_unstable();
    let mut cmap = Vec::new();
    u16s(&mut cmap, &[0, 1, 3, 10]);
    u32s(&mut cmap, &[12]);
    u16s(&mut cmap, &[12, 0]);
    u32s(&mut cmap, &[16 + 12 * mapped.len() as u32, 0, mapped.len() as u32]);
    for (c, gid) in &mapped {
        u32s(&mut cmap, &[*c, *c, *gid]);
    }

    let mut tables = vec![(*b"OS/2", os2), (*b"cmap", cmap), (*b"glyf", glyf), (*b"head", head), (*b"hhea", hhea), (*b"hmtx", hmtx), (*b"loca", loca), (*b"maxp", maxp), (*b"name", names(if bold { "Bold" } else { "Regular" }))];
    tables.sort_by_key(|(tag, _)| *tag);
    let mut out = Vec::new();
    u32s(&mut out, &[0x0001_0000]);
    u16s(&mut out, &[tables.len() as u16, 0, 0, 0]);
    let mut body = Vec::new();
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        u32s(&mut out, &[0, (12 + 16 * tables.len() + body.len()) as u32, data.len() as u32]);
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    out.extend(body);
    out
}

#[test]
fn the_test_fonts_are_what_this_makes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fonts");
    for (name, bold) in [("InkTest-Regular.ttf", false), ("InkTest-Bold.ttf", true)] {
        let (path, made) = (dir.join(name), font(bold));
        if std::env::var_os("INK_BLESS").is_some() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &made).unwrap();
        }
        let kept = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} (INK_BLESS=1 makes it)", path.display()));
        assert!(kept == made, "{name} isn't what tests/font.rs makes: INK_BLESS=1 cargo test -p ink-doc --test font");
    }
}

/// The engine reads them as what the table above says.
#[test]
fn the_test_fonts_set_text_as_their_table_says() {
    use ink_doc::fonts::{self, Face, Family};
    fonts::add(font(false)).unwrap();
    fonts::add(font(true)).unwrap();
    assert!(fonts::has("ink test") && fonts::families().contains(&"Ink Test".to_owned()));
    let face = |bold: bool| Face { family: Family::Named("Ink Test".into()), bold, italic: false };
    let line = fonts::shape("Hi po", &face(false));
    assert_eq!((line.width, line.ascent, line.descent), (2.4, 0.8, 0.2));
    assert_eq!(line.glyphs.iter().map(|g| (g.at, g.place.x, g.advance)).collect::<Vec<_>>(), [(0, 0.0, 0.6), (1, 0.6, 0.3), (2, 0.9, 0.3), (3, 1.2, 0.6), (4, 1.8, 0.6)]);
    let data: Vec<String> = line.glyphs.iter().map(|g| g.outline.to_data(3)).collect();
    assert_eq!(data[0], "M0.1 0 V-0.7 H0.5 V0 Z", "a capital: y runs down, its box closed");
    assert_eq!(data[1], "M0.1 0 V-0.7 H0.2 V0 Z");
    assert_eq!(data[2], "", "a space draws nothing");
    assert_eq!(data[3], "M0.1 0.2 V-0.5 H0.5 V0.2 Z", "a descender reaches below the line");
    assert_eq!(data[4], "M0.3 -0.7 Q0.55 -0.7 0.55 -0.35 Q0.55 0 0.3 0 Q0.05 0 0.05 -0.35 Q0.05 -0.7 0.3 -0.7 Z M0.3 -0.55 Q0.15 -0.55 0.15 -0.35 Q0.15 -0.15 0.3 -0.15 Q0.45 -0.15 0.45 -0.35 Q0.45 -0.55 0.3 -0.55 Z");
    assert_eq!(fonts::shape("H", &face(true)).glyphs[0].outline.to_data(3), "M0.05 0 V-0.7 H0.55 V0 Z", "the bold face is wider");
    assert_eq!(fonts::x_height(&face(false)), 0.5);
}
