//! The icons as data: SVG paths from the design, drawn by the Windows layer.

use crate::scene::Icon;
use crate::theme::Role;

/// One mark of an icon, in the icon's own view box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    Stroke {
        d: &'static str,
        width: f32,
    },
    Fill {
        d: &'static str,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    RoundRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        width: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub prim: Prim,
    /// A colour of its own; `None` draws in the colour the icon is given.
    pub role: Option<Role>,
}

const fn mark(prim: Prim) -> Mark {
    Mark { prim, role: None }
}

const U_SHAPE: &str = "M3 2.5v6.5a5 5 0 0 0 10 0V2.5";
const LIQUID: &str = "M4.4 8.2c1.2-.7 2.4.7 3.6 0s2.4-.7 3.6 0V9a3.6 3.6 0 0 1-7.2 0z";
const PIN: &str = "M10.2 1.8l4 4-2.1.8-2.6 2.6-.3 3-1.1 1.1-2.7-2.7L2 14l-.1-.1 3.3-3.4-2.7-2.7 1.1-1.1 3-.3 2.6-2.6z";

/// The size of the square view box the marks are written for, and the marks.
pub fn marks(icon: Icon) -> (f32, Vec<Mark>) {
    match icon {
        Icon::Container { liquid } => {
            let mut marks = vec![mark(Prim::Stroke {
                d: U_SHAPE,
                width: 1.6,
            })];
            if liquid {
                marks.push(Mark {
                    prim: Prim::Fill { d: LIQUID },
                    role: Some(Role::Mark),
                });
            }
            (16.0, marks)
        }
        Icon::Pin { filled: true } => (16.0, vec![mark(Prim::Fill { d: PIN })]),
        Icon::Pin { filled: false } => (16.0, vec![mark(Prim::Stroke { d: PIN, width: 1.2 })]),
        Icon::MoreDots => (
            16.0,
            [3.5, 8.0, 12.5]
                .into_iter()
                .map(|cx| {
                    mark(Prim::Circle {
                        cx,
                        cy: 8.0,
                        r: 1.2,
                    })
                })
                .collect(),
        ),
        Icon::Close => (
            16.0,
            vec![mark(Prim::Stroke {
                d: "M4 4l8 8M12 4l-8 8",
                width: 1.3,
            })],
        ),
        Icon::Expand => (
            16.0,
            vec![mark(Prim::Stroke {
                d: "M9.5 2.5h4v4M6.5 13.5h-4v-4M13.5 2.5L9 7M2.5 13.5L7 9",
                width: 1.3,
            })],
        ),
        Icon::Copy => (
            16.0,
            vec![
                mark(Prim::RoundRect {
                    x: 5.0,
                    y: 5.0,
                    w: 9.0,
                    h: 9.0,
                    r: 1.5,
                    width: 1.3,
                }),
                mark(Prim::Stroke {
                    d: "M11 3.5V3a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1h.5",
                    width: 1.3,
                }),
            ],
        ),
        Icon::Warning => (
            12.0,
            vec![
                mark(Prim::Stroke {
                    d: "M6 1.2l5 9.3H1z",
                    width: 1.2,
                }),
                mark(Prim::Stroke {
                    d: "M6 4.6v2.8",
                    width: 1.2,
                }),
                mark(Prim::Circle {
                    cx: 6.0,
                    cy: 8.9,
                    r: 0.7,
                }),
            ],
        ),
        Icon::PlugOff => (
            20.0,
            vec![
                mark(Prim::Stroke {
                    d: "M7 3v4M13 3v4M5 7h10v3a5 5 0 0 1-10 0zM10 15v3",
                    width: 1.5,
                }),
                mark(Prim::Stroke {
                    d: "M3 3l14 14",
                    width: 1.5,
                }),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svgpath::parse;

    #[test]
    fn every_path_of_every_icon_parses() {
        let icons = [
            Icon::Container { liquid: true },
            Icon::Container { liquid: false },
            Icon::Pin { filled: true },
            Icon::Pin { filled: false },
            Icon::MoreDots,
            Icon::Close,
            Icon::Expand,
            Icon::Copy,
            Icon::Warning,
            Icon::PlugOff,
        ];
        for icon in icons {
            let (view_box, marks) = marks(icon);
            assert!(view_box > 0.0 && !marks.is_empty(), "{icon:?}");
            for mark in marks {
                if let Prim::Stroke { d, .. } | Prim::Fill { d } = mark.prim {
                    let segs = parse(d).unwrap_or_else(|error| panic!("{icon:?} {d}: {error:?}"));
                    assert!(!segs.is_empty());
                }
            }
        }
    }

    #[test]
    fn the_liquid_is_drawn_in_the_mark_colour_and_only_when_asked() {
        let (_, with) = marks(Icon::Container { liquid: true });
        let (_, without) = marks(Icon::Container { liquid: false });
        assert_eq!(
            with.iter().filter(|m| m.role == Some(Role::Mark)).count(),
            1
        );
        assert_eq!(without.len(), 1);
    }
}
