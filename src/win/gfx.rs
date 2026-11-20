//! Draws a [`Scene`] with Direct2D.

use std::rc::Rc;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED,
    D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINE_JOIN_ROUND, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, D2D1CreateFactory, ID2D1Factory, ID2D1GeometrySink,
    ID2D1HwndRenderTarget, ID2D1PathGeometry, ID2D1RenderTarget, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::core::{Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

use super::text::Typography;
use crate::icons::{Mark, Prim, marks};
use crate::scene::{DotFill, Icon, Rect, Scene, Shape, Text};
use crate::svgpath::{Seg, parse};
use crate::theme::{Color, Mode, Role};

pub fn d2d_color(color: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: color.a,
    }
}

fn point(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn d2d_rect(rect: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: rect.x,
        top: rect.y,
        right: rect.right(),
        bottom: rect.bottom(),
    }
}

fn identity() -> Matrix3x2 {
    Matrix3x2 {
        M11: 1.0,
        M12: 0.0,
        M21: 0.0,
        M22: 1.0,
        M31: 0.0,
        M32: 0.0,
    }
}

/// What it draws on, and the brushes it draws with.
pub struct Painter {
    pub factory: ID2D1Factory,
    target: ID2D1RenderTarget,
    brush: ID2D1SolidColorBrush,
    stroke: ID2D1StrokeStyle,
    text: Rc<Typography>,
    mode: Mode,
}

impl Painter {
    pub fn new(
        factory: &ID2D1Factory,
        target: ID2D1RenderTarget,
        text: Rc<Typography>,
    ) -> Result<Self> {
        unsafe {
            let brush = target.CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)?;
            let stroke = factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                },
                None,
            )?;
            Ok(Self {
                factory: factory.clone(),
                target,
                brush,
                stroke,
                text,
                mode: Mode::Light,
            })
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    fn use_color(&self, role: Role) -> &ID2D1SolidColorBrush {
        unsafe {
            self.brush.SetColor(&d2d_color(self.mode.color(role)));
        }
        &self.brush
    }

    pub fn draw_shapes(&self, shapes: &[Shape], origin: (f32, f32)) {
        for shape in shapes {
            self.draw_shape(shape, origin);
        }
    }

    fn draw_shape(&self, shape: &Shape, origin: (f32, f32)) {
        let (ox, oy) = origin;
        match shape {
            Shape::Rect {
                rect,
                radius,
                fill,
                border,
            } => self.draw_rect(rect.translated(ox, oy), *radius, *fill, *border),
            Shape::Text(text) => self.draw_text(text, origin),
            Shape::Ring {
                cx,
                cy,
                radius,
                width,
                color,
                sweep,
            } => self.draw_ring(cx + ox, cy + oy, *radius, *width, *color, *sweep),
            Shape::Dot {
                cx,
                cy,
                diameter,
                border,
                color,
                fill,
            } => self.draw_dot(cx + ox, cy + oy, *diameter, *border, *color, *fill),
            Shape::Icon { icon, rect, color } => {
                self.draw_icon(*icon, rect.translated(ox, oy), *color)
            }
        }
    }

    fn draw_rect(&self, rect: Rect, radius: f32, fill: Option<Role>, border: Option<(Role, f32)>) {
        unsafe {
            if let Some(role) = fill {
                if radius > 0.0 {
                    let rounded = D2D1_ROUNDED_RECT {
                        rect: d2d_rect(rect),
                        radiusX: radius,
                        radiusY: radius,
                    };
                    self.target
                        .FillRoundedRectangle(&rounded, self.use_color(role));
                } else {
                    self.target
                        .FillRectangle(&d2d_rect(rect), self.use_color(role));
                }
            }
            if let Some((role, width)) = border {
                let inset = width / 2.0;
                let rounded = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(Rect::new(
                        rect.x + inset,
                        rect.y + inset,
                        rect.w - width,
                        rect.h - width,
                    )),
                    radiusX: (radius - inset).max(0.0),
                    radiusY: (radius - inset).max(0.0),
                };
                self.target
                    .DrawRoundedRectangle(&rounded, self.use_color(role), width, None);
            }
        }
    }

    fn draw_text(&self, text: &Text, origin: (f32, f32)) {
        let Ok(format) = self.text.format(text.font, text.align, text.ellipsis) else {
            return;
        };
        let chars: Vec<u16> = text.text.encode_utf16().collect();
        let rect = d2d_rect(text.rect.translated(origin.0, origin.1));
        unsafe {
            self.target.DrawText(
                &chars,
                &format,
                &rect,
                self.use_color(text.color),
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn draw_ring(&self, cx: f32, cy: f32, radius: f32, width: f32, role: Role, sweep: f32) {
        unsafe {
            if sweep >= 0.999 {
                let ellipse = D2D1_ELLIPSE {
                    point: point(cx, cy),
                    radiusX: radius,
                    radiusY: radius,
                };
                self.target
                    .DrawEllipse(&ellipse, self.use_color(role), width, &self.stroke);
                return;
            }
            let angle = sweep.clamp(0.0, 1.0) * std::f32::consts::TAU;
            let Ok(geometry) = self.arc_geometry(cx, cy, radius, angle, false) else {
                return;
            };
            self.target
                .DrawGeometry(&geometry, self.use_color(role), width, &self.stroke);
        }
    }

    /// An arc from 12 o'clock clockwise by `angle`, or the pie slice of it.
    fn arc_geometry(
        &self,
        cx: f32,
        cy: f32,
        radius: f32,
        angle: f32,
        pie: bool,
    ) -> Result<ID2D1PathGeometry> {
        unsafe {
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            let start = point(cx, cy - radius);
            let end = point(cx + radius * angle.sin(), cy - radius * angle.cos());
            if pie {
                sink.BeginFigure(point(cx, cy), D2D1_FIGURE_BEGIN_FILLED);
                sink.AddLine(start);
            } else {
                sink.BeginFigure(start, D2D1_FIGURE_BEGIN_HOLLOW);
            }
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: end,
                size: D2D_SIZE_F {
                    width: radius,
                    height: radius,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if angle > std::f32::consts::PI {
                    D2D1_ARC_SIZE_LARGE
                } else {
                    D2D1_ARC_SIZE_SMALL
                },
            });
            sink.EndFigure(if pie {
                D2D1_FIGURE_END_CLOSED
            } else {
                D2D1_FIGURE_END_OPEN
            });
            sink.Close()?;
            Ok(geometry)
        }
    }

    fn draw_dot(&self, cx: f32, cy: f32, diameter: f32, border: f32, role: Role, fill: DotFill) {
        unsafe {
            let radius = diameter / 2.0;
            let center = point(cx, cy);
            match fill {
                DotFill::Full => {
                    let ellipse = D2D1_ELLIPSE {
                        point: center,
                        radiusX: radius,
                        radiusY: radius,
                    };
                    self.target.FillEllipse(&ellipse, self.use_color(role));
                }
                DotFill::Quarter | DotFill::ThreeQuarters => {
                    let share = if fill == DotFill::Quarter { 0.25 } else { 0.75 };
                    let angle = share * std::f32::consts::TAU;
                    if let Ok(pie) = self.arc_geometry(cx, cy, radius, angle, true) {
                        self.target.FillGeometry(&pie, self.use_color(role), None);
                    }
                }
                DotFill::Empty => {}
            }
            // The ring is inside the dot's box, as a CSS border is.
            let ring = D2D1_ELLIPSE {
                point: center,
                radiusX: radius - border / 2.0,
                radiusY: radius - border / 2.0,
            };
            self.target
                .DrawEllipse(&ring, self.use_color(role), border, None);
        }
    }

    fn draw_icon(&self, icon: Icon, rect: Rect, role: Role) {
        let (view_box, marks) = marks(icon);
        let scale = rect.w / view_box;
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: scale,
                M12: 0.0,
                M21: 0.0,
                M22: scale,
                M31: rect.x,
                M32: rect.y,
            });
            for Mark { prim, role: own } in marks {
                let role = own.unwrap_or(role);
                self.draw_mark(prim, role);
            }
            self.target.SetTransform(&identity());
        }
    }

    fn draw_mark(&self, prim: Prim, role: Role) {
        unsafe {
            match prim {
                Prim::Stroke { d, width } => {
                    if let Ok(geometry) = self.path_geometry(d, false) {
                        self.target.DrawGeometry(
                            &geometry,
                            self.use_color(role),
                            width,
                            &self.stroke,
                        );
                    }
                }
                Prim::Fill { d } => {
                    if let Ok(geometry) = self.path_geometry(d, true) {
                        self.target
                            .FillGeometry(&geometry, self.use_color(role), None);
                    }
                }
                Prim::Circle { cx, cy, r } => {
                    let ellipse = D2D1_ELLIPSE {
                        point: point(cx, cy),
                        radiusX: r,
                        radiusY: r,
                    };
                    self.target.FillEllipse(&ellipse, self.use_color(role));
                }
                Prim::RoundRect {
                    x,
                    y,
                    w,
                    h,
                    r,
                    width,
                } => {
                    let rounded = D2D1_ROUNDED_RECT {
                        rect: d2d_rect(Rect::new(x, y, w, h)),
                        radiusX: r,
                        radiusY: r,
                    };
                    self.target.DrawRoundedRectangle(
                        &rounded,
                        self.use_color(role),
                        width,
                        &self.stroke,
                    );
                }
            }
        }
    }

    /// An SVG path as a geometry, to fill (every figure closed) or stroke.
    fn path_geometry(&self, d: &str, filled: bool) -> Result<ID2D1PathGeometry> {
        let segs = parse(d).map_err(|_| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)
        })?;
        unsafe {
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            let begin = if filled {
                D2D1_FIGURE_BEGIN_FILLED
            } else {
                D2D1_FIGURE_BEGIN_HOLLOW
            };
            let mut open = false;
            for seg in segs {
                match seg {
                    Seg::Move(x, y) => {
                        end_figure(&sink, &mut open, filled);
                        sink.BeginFigure(point(x, y), begin);
                        open = true;
                    }
                    Seg::Line(x, y) => sink.AddLine(point(x, y)),
                    Seg::Cubic { c1, c2, to } => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: point(c1.0, c1.1),
                        point2: point(c2.0, c2.1),
                        point3: point(to.0, to.1),
                    }),
                    Seg::Arc {
                        to,
                        radius,
                        rotation,
                        large,
                        sweep,
                    } => sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: point(to.0, to.1),
                        size: D2D_SIZE_F {
                            width: radius.0,
                            height: radius.1,
                        },
                        rotationAngle: rotation,
                        sweepDirection: if sweep {
                            D2D1_SWEEP_DIRECTION_CLOCKWISE
                        } else {
                            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                        },
                        arcSize: if large {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        },
                    }),
                    Seg::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                    }
                }
            }
            end_figure(&sink, &mut open, filled);
            sink.Close()?;
            Ok(geometry)
        }
    }

    pub fn begin(&self) {
        unsafe {
            self.target.BeginDraw();
        }
    }

    pub fn end(&self) -> Result<()> {
        unsafe { self.target.EndDraw(None, None) }
    }

    pub fn clear(&self, role: Role) {
        unsafe {
            self.target.Clear(Some(&d2d_color(self.mode.color(role))));
        }
    }

    pub fn clip(&self, rect: Rect) {
        unsafe {
            self.target
                .PushAxisAlignedClip(&d2d_rect(rect), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn unclip(&self) {
        unsafe {
            self.target.PopAxisAlignedClip();
        }
    }
}

fn end_figure(sink: &ID2D1GeometrySink, open: &mut bool, filled: bool) {
    if *open {
        unsafe {
            sink.EndFigure(if filled {
                D2D1_FIGURE_END_CLOSED
            } else {
                D2D1_FIGURE_END_OPEN
            });
        }
        *open = false;
    }
}

/// The window's drawing surface.
pub struct Renderer {
    target: ID2D1HwndRenderTarget,
    painter: Painter,
}

impl Renderer {
    pub fn new(hwnd: HWND, size: (u32, u32), dpi: u32, text: Rc<Typography>) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let window = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: size.0.max(1),
                    height: size.1.max(1),
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let target = factory.CreateHwndRenderTarget(&properties, &window)?;
            target.SetDpi(dpi as f32, dpi as f32);
            let painter = Painter::new(&factory, target.cast()?, text)?;
            Ok(Self { target, painter })
        }
    }

    pub fn resize(&self, size: (u32, u32)) -> Result<()> {
        unsafe {
            self.target.Resize(&D2D_SIZE_U {
                width: size.0.max(1),
                height: size.1.max(1),
            })
        }
    }

    pub fn set_dpi(&self, dpi: u32) {
        unsafe {
            self.target.SetDpi(dpi as f32, dpi as f32);
        }
    }

    /// Draws the window. An error with `D2DERR_RECREATE_TARGET` means the
    /// surface was lost and a new renderer is needed.
    pub fn draw(&mut self, scene: &Scene, mode: Mode, scroll: f32) -> Result<()> {
        self.painter.set_mode(mode);
        let painter = &self.painter;
        painter.begin();
        painter.clear(Role::Win);
        painter.draw_shapes(&scene.title, (0.0, 0.0));
        painter.clip(Rect::new(
            crate::scene::BORDER,
            scene.content_top,
            scene.width - 2.0 * crate::scene::BORDER,
            scene.view_height,
        ));
        painter.draw_shapes(
            &scene.content,
            (crate::scene::BORDER, scene.content_top - scroll),
        );
        painter.unclip();
        // The frame line goes on top, rounded like the window.
        painter.draw_shapes(
            &[Shape::Rect {
                rect: Rect::new(0.0, 0.0, scene.width, scene.height),
                radius: 8.0,
                fill: None,
                border: Some((Role::WinBorder, crate::scene::BORDER)),
            }],
            (0.0, 0.0),
        );
        painter.end()
    }
}
