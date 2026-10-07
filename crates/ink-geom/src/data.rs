//! SVG path data (`d="M0 0 L…"`): read into a [`Path`], every command,
//! relative and absolute, and written back from one.

use lntrn_math::Vec2;

use crate::arc::{Shape, shape};
use crate::number::{self, MAX_DECIMALS, scan, skip_separators};
use crate::path::{ArcTo, Path, Seg};

/// A path read from path data.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub path: Path,
    /// Where in the text reading gave up, when not all of it was path
    /// data. The path holds what came before, which is what SVG says to
    /// draw.
    pub stopped_at: Option<usize>,
}

/// An arc flag: one digit, which may run straight into what follows
/// (`a1 1 0 01.5.5`).
fn flag(s: &[u8], i: &mut usize) -> Option<bool> {
    let before = *i;
    skip_separators(s, i);
    match s.get(*i) {
        Some(b'0') | Some(b'1') => {
            *i += 1;
            Some(s[*i - 1] == b'1')
        }
        _ => {
            *i = before;
            None
        }
    }
}

/// An x and a y.
fn pair(s: &[u8], i: &mut usize) -> Option<Vec2> {
    Some(Vec2::new(scan(s, i)?, scan(s, i)?))
}

/// `c` mirrored through `p` (the control point of a smooth curve).
fn mirror(p: Vec2, c: Vec2) -> Vec2 {
    p * 2.0 - c
}

/// The middle of the arc from `from` to `to`, if it is one.
fn apex(from: Vec2, arc: &ArcTo, to: Vec2) -> Option<Vec2> {
    match shape(from, arc, to) {
        Shape::Arc(c) => Some(c.at(c.theta + c.delta / 2.0, 1.0)),
        _ => None,
    }
}

/// An arc's radii as they're written, to `decimals` places or as many
/// more as it takes. An arc is given by its ends, so its ends rounded a
/// hair closer together (or a radius rounded a hair up) pull it flat,
/// by far more than the rounding: twenty times, near a half turn. So
/// the radii written are the first of these that keep the arc's middle
/// where it was meant, within one unit of the last place: to the
/// nearest; rounded down (radii that fall short of the ends grow to
/// just reach them, which is a half turn exactly); then the same with
/// one more decimal, and so on.
fn radii(from: Vec2, arc: &ArcTo, to: Vec2, decimals: usize) -> (String, String) {
    let n = |v: f64, places: usize| number::format(v, places);
    let read = |text: &str| number::parse(text).unwrap_or(0.0);
    let plain = (n(arc.rx, decimals), n(arc.ry, decimals));
    let written = |p: Vec2| Vec2::new(read(&n(p.x, decimals)), read(&n(p.y, decimals)));
    let (w_from, w_to, rotation) = (written(from), written(to), read(&n(arc.rotation, decimals)));
    // What's meant: the same sweep of the same ellipse, between the
    // ends as they're written. In the ellipse's own frame (unturned,
    // its radii 1) a sweep's chord is 2 sin(sweep / 2) long.
    let Shape::Arc(meant) = shape(from, arc, to) else { return plain };
    let (sin, cos) = rotation.to_radians().sin_cos();
    let chord = (w_to - w_from) * 0.5;
    let half = ((cos * chord.x + sin * chord.y) / meant.rx).hypot((cos * chord.y - sin * chord.x) / meant.ry);
    let fit = half / (meant.delta.abs() * 0.5).sin();
    let exact = ArcTo { rx: meant.rx * fit, ry: meant.ry * fit, rotation, ..*arc };
    let Some(middle) = apex(w_from, &exact, w_to).filter(|_| fit.is_finite() && fit > 0.0) else { return plain };
    let within = 10f64.powi(-(decimals as i32));
    for places in decimals..=MAX_DECIMALS {
        let unit = 10f64.powi(places as i32);
        for down in [false, true] {
            let cut = |v: f64| n(if down { (v * unit).floor() / unit } else { v }, places);
            let (rx, ry) = if places == decimals && !down { plain.clone() } else { (cut(exact.rx), cut(exact.ry)) };
            let tried = ArcTo { rx: read(&rx), ry: read(&ry), rotation, ..*arc };
            if apex(w_from, &tried, w_to).is_some_and(|at| at.distance(middle) <= within) {
                return (rx, ry);
            }
        }
    }
    plain
}

impl Path {
    /// Read path data. Coordinates come out absolute, `H` and `V` as
    /// lines, and the smooth curves (`S`, `T`) with the control point
    /// they imply. Reading stops at the first thing that isn't path data.
    pub fn parse(d: &str) -> Parsed {
        let s = d.as_bytes();
        let mut i = 0;
        let mut path = Path::new();
        let mut cmd = 0u8;
        let (mut pos, mut start) = (Vec2::ZERO, Vec2::ZERO);
        // The last curve's second control point, while the next command
        // could be a smooth curve of its kind.
        let (mut last_cubic, mut last_quad): (Option<Vec2>, Option<Vec2>) = (None, None);
        let stopped_at = loop {
            skip_separators(s, &mut i);
            if i >= s.len() {
                break None;
            }
            let at = i;
            if s[i].is_ascii_alphabetic() {
                cmd = s[i];
                i += 1;
            } else if cmd == 0 || cmd.eq_ignore_ascii_case(&b'Z') {
                // Numbers before any command, or after a close.
                break Some(at);
            }
            // Path data starts with a move.
            if path.is_empty() && !cmd.eq_ignore_ascii_case(&b'M') {
                break Some(at);
            }
            let rel = cmd.is_ascii_lowercase();
            let base = if rel { pos } else { Vec2::ZERO };
            let (mut cubic_ctrl, mut quad_ctrl) = (None, None);
            let read = match cmd.to_ascii_uppercase() {
                b'M' => pair(s, &mut i).map(|p| {
                    pos = base + p;
                    start = pos;
                    path.move_to(pos);
                    // Further pairs are line-tos.
                    cmd = if rel { b'l' } else { b'L' };
                }),
                b'L' => pair(s, &mut i).map(|p| {
                    pos = base + p;
                    path.line_to(pos);
                }),
                b'H' => scan(s, &mut i).map(|x| {
                    pos = Vec2::new(base.x + x, pos.y);
                    path.line_to(pos);
                }),
                b'V' => scan(s, &mut i).map(|y| {
                    pos = Vec2::new(pos.x, base.y + y);
                    path.line_to(pos);
                }),
                b'C' => (|| Some((pair(s, &mut i)?, pair(s, &mut i)?, pair(s, &mut i)?)))().map(|(c1, c2, to)| {
                    path.cubic_to(base + c1, base + c2, base + to);
                    cubic_ctrl = Some(base + c2);
                    pos = base + to;
                }),
                b'S' => (|| Some((pair(s, &mut i)?, pair(s, &mut i)?)))().map(|(c2, to)| {
                    path.cubic_to(last_cubic.map_or(pos, |c| mirror(pos, c)), base + c2, base + to);
                    cubic_ctrl = Some(base + c2);
                    pos = base + to;
                }),
                b'Q' => (|| Some((pair(s, &mut i)?, pair(s, &mut i)?)))().map(|(c, to)| {
                    path.quad_to(base + c, base + to);
                    quad_ctrl = Some(base + c);
                    pos = base + to;
                }),
                b'T' => pair(s, &mut i).map(|to| {
                    let c = last_quad.map_or(pos, |c| mirror(pos, c));
                    path.quad_to(c, base + to);
                    quad_ctrl = Some(c);
                    pos = base + to;
                }),
                b'A' => (|| Some((scan(s, &mut i)?, scan(s, &mut i)?, scan(s, &mut i)?, flag(s, &mut i)?, flag(s, &mut i)?, pair(s, &mut i)?)))().map(|(rx, ry, rotation, large, sweep, to)| {
                    path.arc_to(ArcTo { rx: rx.abs(), ry: ry.abs(), rotation, large, sweep }, base + to);
                    pos = base + to;
                }),
                b'Z' => {
                    path.close();
                    pos = start;
                    Some(())
                }
                _ => None,
            };
            if read.is_none() {
                break Some(at);
            }
            (last_cubic, last_quad) = (cubic_ctrl, quad_ctrl);
        };
        Parsed { path, stopped_at }
    }

    /// The path as path data, numbers to `decimals` places: absolute
    /// commands, `H` and `V` where a line is level or upright. An arc's
    /// radii take more places where it would otherwise be drawn
    /// somewhere else (see [`radii`]).
    pub fn to_data(&self, decimals: usize) -> String {
        let n = |v: f64| number::format(v, decimals);
        let xy = |p: Vec2| format!("{} {}", n(p.x), n(p.y));
        let mut parts: Vec<String> = Vec::new();
        for sub in &self.subpaths {
            parts.push(format!("M{}", xy(sub.start)));
            let mut at = sub.start;
            for seg in &sub.segs {
                parts.push(match *seg {
                    // By what's written, so the pen stays where the
                    // numbers say it is.
                    Seg::Line { to } if n(to.y) == n(at.y) && n(to.x) != n(at.x) => format!("H{}", n(to.x)),
                    Seg::Line { to } if n(to.x) == n(at.x) && n(to.y) != n(at.y) => format!("V{}", n(to.y)),
                    Seg::Line { to } => format!("L{}", xy(to)),
                    Seg::Quad { c, to } => format!("Q{} {}", xy(c), xy(to)),
                    Seg::Cubic { c1, c2, to } => format!("C{} {} {}", xy(c1), xy(c2), xy(to)),
                    Seg::Arc { arc, to } => {
                        let (rx, ry) = radii(at, &arc, to, decimals);
                        format!("A{rx} {ry} {} {} {} {}", n(arc.rotation), arc.large as u8, arc.sweep as u8, xy(to))
                    }
                });
                at = seg.to();
            }
            if sub.closed {
                parts.push("Z".to_owned());
            }
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn read(d: &str) -> Path {
        let parsed = Path::parse(d);
        assert_eq!(parsed.stopped_at, None, "{d}");
        parsed.path
    }

    #[test]
    fn reads_every_command_relative_and_absolute() {
        let p = read("M0 0h10v10H0z");
        assert_eq!(p.subpaths.len(), 1);
        assert!(p.subpaths[0].closed);
        assert_eq!(p.subpaths[0].segs, vec![Seg::Line { to: v(10.0, 0.0) }, Seg::Line { to: v(10.0, 10.0) }, Seg::Line { to: v(0.0, 10.0) }]);
        let p = read("m1-.5.5.5");
        assert_eq!((p.subpaths[0].start, p.subpaths[0].segs[0]), (v(1.0, -0.5), Seg::Line { to: v(1.5, 0.0) }), "pairs after a move are relative lines");
        let p = read("M10 10 C 10 20, 20 20, 20 10 s 10 -10 10 0 Q 35 0 40 10 t 10 0");
        assert_eq!(
            p.subpaths[0].segs,
            vec![
                Seg::Cubic { c1: v(10.0, 20.0), c2: v(20.0, 20.0), to: v(20.0, 10.0) },
                Seg::Cubic { c1: v(20.0, 0.0), c2: v(30.0, 0.0), to: v(30.0, 10.0) },
                Seg::Quad { c: v(35.0, 0.0), to: v(40.0, 10.0) },
                Seg::Quad { c: v(45.0, 20.0), to: v(50.0, 10.0) },
            ],
            "smooth curves mirror the control point before them"
        );
        // A smooth curve after anything else starts from the pen.
        let p = read("M0 0 L5 5 S 8 8 10 5 T 20 5");
        assert_eq!(p.subpaths[0].segs[1], Seg::Cubic { c1: v(5.0, 5.0), c2: v(8.0, 8.0), to: v(10.0, 5.0) });
        assert_eq!(p.subpaths[0].segs[2], Seg::Quad { c: v(10.0, 5.0), to: v(20.0, 5.0) });
    }

    #[test]
    fn reads_arcs_with_their_flags_run_together() {
        let p = read("M0 0a5 5 0 0110 0A-3 4 30 1,0 2 2");
        assert_eq!(p.subpaths[0].segs[0], Seg::Arc { arc: ArcTo { rx: 5.0, ry: 5.0, rotation: 0.0, large: false, sweep: true }, to: v(10.0, 0.0) });
        assert_eq!(p.subpaths[0].segs[1], Seg::Arc { arc: ArcTo { rx: 3.0, ry: 4.0, rotation: 30.0, large: true, sweep: false }, to: v(2.0, 2.0) });
    }

    #[test]
    fn subpaths_follow_moves_and_closes() {
        let p = read("M0 0L1 0 1 1M5 5l1 0");
        assert_eq!(p.subpaths.len(), 2, "implicit line-tos after M, then a second subpath");
        assert!(!p.subpaths[0].closed && !p.subpaths[1].closed);
        // After Z, drawing without a move starts where the subpath did.
        let p = read("M2 3h4v4zl-5 0");
        assert_eq!(p.subpaths.len(), 2);
        assert_eq!((p.subpaths[1].start, p.subpaths[1].segs[0]), (v(2.0, 3.0), Seg::Line { to: v(-3.0, 3.0) }));
        // A lone move is kept: it's what the file says.
        assert_eq!(read("M5 5").subpaths.len(), 1);
        assert!(read("").is_empty() && read("  ").is_empty());
    }

    #[test]
    fn reading_stops_at_what_is_not_path_data() {
        let stopped = |d: &str| {
            let p = Path::parse(d);
            (p.path.subpaths.iter().map(|s| s.segs.len()).sum::<usize>(), p.stopped_at)
        };
        assert_eq!(stopped("M0 0L10 0L1e999 5L0 10"), (1, Some(9)), "a number past f64's range: the command it's in");
        assert_eq!(stopped("M0 0 L 5"), (0, Some(5)), "half a pair");
        assert_eq!(stopped("M0 0 X 5 5"), (0, Some(5)), "no such command");
        assert_eq!(stopped("L5 5"), (0, Some(0)), "path data starts with a move");
        assert_eq!(stopped("5 5 L1 1"), (0, Some(0)));
        assert_eq!(stopped("M0 0 L1 1 Z 5 5"), (1, Some(12)), "numbers after a close");
        assert_eq!(stopped("M0 0 A5 5 0 2 1 10 0"), (0, Some(5)), "a flag is 0 or 1");
    }

    #[test]
    fn writes_what_it_reads() {
        for d in ["M0 0 H10 V10 H0 Z", "M1 -0.5 L1.5 0", "M10 10 C10 20 20 20 20 10 Q35 0 40 10", "M0 0 A5 5 0 0 1 10 0 A3 4 30 1 0 2 2 Z M5 5 L6 7"] {
            let p = read(d);
            assert_eq!(p.to_data(3), d);
            assert_eq!(read(&p.to_data(3)), p);
        }
        // Rounded to the places asked for; level and upright by what's written.
        let p = read("M0.12345 0 L10 0.0001 L10.0004 5");
        assert_eq!(p.to_data(3), "M0.123 0 H10 V5");
        assert_eq!(p.to_data(0), "M0 0 H10 V5");
        // A line that goes nowhere is still written as a line.
        assert_eq!(read("M1 1 L1 1").to_data(3), "M1 1 L1 1");
    }

    #[test]
    fn an_arc_is_written_so_that_it_stays_where_it_was() {
        // How far the middle of `d`'s first arc is from the same arc's
        // as written to three places.
        let moved = |path: &Path| {
            let middle = |p: &Path| match p.subpaths[0].segs[0] {
                Seg::Arc { ref arc, to } => apex(p.subpaths[0].start, arc, to).unwrap(),
                _ => panic!(),
            };
            (path.to_data(3), middle(path).distance(middle(&read(&path.to_data(3)))))
        };
        // A half circle, its ends turned to where three places put them
        // a hair closer together than its radius reaches: written with
        // the radius cut short, which grows to a half turn exactly.
        let turned = read("M8 7 A4 4 0 0 1 16 7").transformed(&crate::Affine::rotate(30f64.to_radians()));
        let (text, off) = moved(&turned);
        assert_eq!(text, "M3.428 10.062 A3.999 3.999 0 0 1 10.356 14.062");
        assert!(off < 2e-3, "{off}");
        // Written to the nearest it would have sagged by twenty times
        // what was rounded away.
        let sagging = read("M3.428 10.062 A4 4 0 0 1 10.356 14.062");
        assert!(apex(sagging.subpaths[0].start, &ArcTo { rx: 4.0, ry: 4.0, rotation: 0.0, large: false, sweep: true }, v(10.356, 14.062)).unwrap().distance(apex(turned.subpaths[0].start, &ArcTo { rx: 4.0, ry: 4.0, rotation: 0.0, large: false, sweep: true }, turned.subpaths[0].segs[0].to()).unwrap()) > 0.01);
        // Nearly a half turn: more places, as many as it takes.
        let nearly = read("M0 0 A5.0031 5.0031 0 0 1 10 0").transformed(&crate::Affine::rotate(1.0));
        let (text, off) = moved(&nearly);
        assert_eq!(text, "M0 0 A5.0032 5.0032 0 0 1 5.403 8.415");
        assert!(off < 2e-3, "{off}");
        // An ordinary arc, and one whose radii never reached: as ever.
        assert_eq!(read("M10 0 A10 10 0 0 1 0 10").transformed(&crate::Affine::rotate(0.3)).to_data(3), "M9.553 2.955 A10 10 0 0 1 -2.955 9.553");
        assert_eq!(read("M0 0 A1 2 30 0 1 10 0").to_data(3), "M0 0 A1 2 30 0 1 10 0");
    }
}
