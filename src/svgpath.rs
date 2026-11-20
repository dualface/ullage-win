//! A parser for the SVG path syntax the icons are written in, producing
//! absolute segments. Only the commands the icons use are understood:
//! `M L H V C S A Z`, in both cases.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic {
        c1: (f32, f32),
        c2: (f32, f32),
        to: (f32, f32),
    },
    Arc {
        to: (f32, f32),
        radius: (f32, f32),
        rotation: f32,
        large: bool,
        sweep: bool,
    },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Index of the character where parsing stopped.
    pub at: usize,
}

struct Scanner {
    chars: Vec<char>,
    pos: usize,
}

impl Scanner {
    fn skip_separators(&mut self) {
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_whitespace() || *c == ',')
        {
            self.pos += 1;
        }
    }

    fn error(&self) -> ParseError {
        ParseError { at: self.pos }
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        self.chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
    }

    /// A number such as `-.7`; a second `.` starts the next number, as in
    /// `2.4.7`.
    fn number(&mut self) -> Result<f32, ParseError> {
        self.skip_separators();
        let start = self.pos;
        if matches!(self.chars.get(self.pos), Some('-' | '+')) {
            self.pos += 1;
        }
        let mut seen_dot = false;
        let mut digits = 0;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_ascii_digit() {
                digits += 1;
            } else if c == '.' && !seen_dot {
                seen_dot = true;
            } else {
                break;
            }
            self.pos += 1;
        }
        if digits == 0 {
            self.pos = start;
            return Err(self.error());
        }
        self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| ParseError { at: start })
    }

    /// An arc flag is a single `0` or `1`, which may touch the next number.
    fn flag(&mut self) -> Result<bool, ParseError> {
        self.skip_separators();
        let flag = match self.chars.get(self.pos) {
            Some('0') => false,
            Some('1') => true,
            _ => return Err(self.error()),
        };
        self.pos += 1;
        Ok(flag)
    }

    fn pair(&mut self) -> Result<(f32, f32), ParseError> {
        Ok((self.number()?, self.number()?))
    }
}

pub fn parse(d: &str) -> Result<Vec<Seg>, ParseError> {
    let mut scan = Scanner {
        chars: d.chars().collect(),
        pos: 0,
    };
    let mut segs = Vec::new();
    let (mut x, mut y) = (0.0_f32, 0.0_f32);
    let (mut start_x, mut start_y) = (0.0_f32, 0.0_f32);
    // The second control point of the previous curve, for `S`.
    let mut last_c2: Option<(f32, f32)> = None;
    let mut command: Option<char> = None;

    loop {
        scan.skip_separators();
        let Some(&c) = scan.chars.get(scan.pos) else {
            break;
        };
        if c.is_ascii_alphabetic() {
            command = Some(c);
            scan.pos += 1;
            if matches!(c, 'Z' | 'z') {
                segs.push(Seg::Close);
                (x, y) = (start_x, start_y);
                last_c2 = None;
                continue;
            }
        } else if !scan.at_number() {
            return Err(scan.error());
        }
        let Some(cmd) = command else {
            return Err(scan.error());
        };
        // A path starts with a move.
        if segs.is_empty() && !matches!(cmd, 'M' | 'm') {
            return Err(scan.error());
        }
        let relative = cmd.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut curve = None;
        match cmd.to_ascii_uppercase() {
            'M' => {
                let (px, py) = scan.pair()?;
                (x, y) = (ox + px, oy + py);
                (start_x, start_y) = (x, y);
                segs.push(Seg::Move(x, y));
                // More pairs after a move are lines.
                command = Some(if relative { 'l' } else { 'L' });
            }
            'L' => {
                let (px, py) = scan.pair()?;
                (x, y) = (ox + px, oy + py);
                segs.push(Seg::Line(x, y));
            }
            'H' => {
                x = ox + scan.number()?;
                segs.push(Seg::Line(x, y));
            }
            'V' => {
                y = oy + scan.number()?;
                segs.push(Seg::Line(x, y));
            }
            'C' => {
                let c1 = scan.pair()?;
                let c2 = scan.pair()?;
                let to = scan.pair()?;
                let (c1, c2, to) = (
                    (ox + c1.0, oy + c1.1),
                    (ox + c2.0, oy + c2.1),
                    (ox + to.0, oy + to.1),
                );
                segs.push(Seg::Cubic { c1, c2, to });
                curve = Some(c2);
                (x, y) = to;
            }
            'S' => {
                let c2 = scan.pair()?;
                let to = scan.pair()?;
                let (c2, to) = ((ox + c2.0, oy + c2.1), (ox + to.0, oy + to.1));
                let c1 = match last_c2 {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                segs.push(Seg::Cubic { c1, c2, to });
                curve = Some(c2);
                (x, y) = to;
            }
            'A' => {
                let radius = scan.pair()?;
                let rotation = scan.number()?;
                let large = scan.flag()?;
                let sweep = scan.flag()?;
                let to = scan.pair()?;
                (x, y) = (ox + to.0, oy + to.1);
                segs.push(Seg::Arc {
                    to: (x, y),
                    radius,
                    rotation,
                    large,
                    sweep,
                });
            }
            _ => return Err(scan.error()),
        }
        last_c2 = curve;
    }
    Ok(segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_lines_and_arcs_become_absolute() {
        let segs = parse("M3 2.5v6.5a5 5 0 0 0 10 0V2.5").unwrap();
        assert_eq!(
            segs,
            [
                Seg::Move(3.0, 2.5),
                Seg::Line(3.0, 9.0),
                Seg::Arc {
                    to: (13.0, 9.0),
                    radius: (5.0, 5.0),
                    rotation: 0.0,
                    large: false,
                    sweep: false
                },
                Seg::Line(13.0, 2.5),
            ]
        );
    }

    #[test]
    fn compact_numbers_split_at_a_second_dot_and_at_signs() {
        let segs = parse("M4.4 8.2c1.2-.7 2.4.7 3.6 0").unwrap();
        let Seg::Cubic { c1, c2, to } = segs[1] else {
            panic!("{segs:?}")
        };
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5;
        assert!(close(c1, (5.6, 7.5)), "{c1:?}");
        assert!(close(c2, (6.8, 8.9)), "{c2:?}");
        assert!(close(to, (8.0, 8.2)), "{to:?}");
    }

    #[test]
    fn a_smooth_curve_reflects_the_previous_control_point() {
        let segs = parse("M0 0C1 1 2 1 3 0S5 -1 6 0").unwrap();
        let Seg::Cubic { c1, c2, to } = segs[2] else {
            panic!("{segs:?}")
        };
        assert_eq!(c1, (4.0, -1.0));
        assert_eq!(c2, (5.0, -1.0));
        assert_eq!(to, (6.0, 0.0));
    }

    #[test]
    fn extra_pairs_after_a_move_are_lines_and_z_closes_back_to_the_start() {
        let segs = parse("M1 1 l4 0 0 4z m2 2h1").unwrap();
        assert_eq!(
            segs,
            [
                Seg::Move(1.0, 1.0),
                Seg::Line(5.0, 1.0),
                Seg::Line(5.0, 5.0),
                Seg::Close,
                Seg::Move(3.0, 3.0),
                Seg::Line(4.0, 3.0),
            ]
        );
    }

    #[test]
    fn arc_flags_may_touch_the_following_number() {
        let segs = parse("M0 0a3.6 3.6 0 0 1-7.2 0").unwrap();
        assert_eq!(
            segs[1],
            Seg::Arc {
                to: (-7.2, 0.0),
                radius: (3.6, 3.6),
                rotation: 0.0,
                large: false,
                sweep: true
            }
        );
        assert!(parse("M0 0a1 1 0 11 5 0").is_ok());
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(parse("M1").is_err());
        assert!(parse("L1 1").is_err());
        assert!(parse("M0 0Q1 1 2 2").is_err());
        assert!(parse("M0 0 a1 1 0 2 0 1 1").is_err());
        assert_eq!(parse("").unwrap(), []);
    }
}
