//! What the pairing dialog needs to draw itself: the fonts and brushes of its
//! controls, the Direct2D painter, and the state of the parts that are drawn
//! (the focus, the button the pointer is over). The shapes themselves are
//! `scene::pair`'s.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory,
    ID2D1Factory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
    DeleteObject, HBRUSH, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SRCCOPY,
};
use windows::core::{Interface, Result};

use super::gfx::Painter;
use super::surface::Surface;
use super::text::Typography;
use crate::scene::pair::{
    ButtonState, CODE_FONT, EDIT_FONT, Focus, Kind, LABEL_FONT, Model, button_shapes, frame,
};
use crate::scene::{Font, Rect, Shape};
use crate::theme::{Color, Mode, Role};

/// The colour of a role as GDI wants it. Only roles without transparency are
/// used for brushes.
pub fn colorref(mode: Mode, role: Role) -> COLORREF {
    let Color { r, g, b, .. } = mode.color(role);
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// A font of `font`'s size in DIPs, for a window at `dpi`.
fn make_font(typography: &Typography, font: Font, dpi: u32) -> HFONT {
    let pixels = (font.size * dpi as f32 / 96.0).round() as i32;
    let family = typography.family_name(font.kind);
    unsafe {
        CreateFontW(
            -pixels,
            0,
            0,
            0,
            i32::from(font.weight),
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            windows::core::PCWSTR(family.as_ptr()),
        )
    }
}

/// The GDI objects of the dialog at one DPI and one mode. They are deleted
/// when it is dropped.
struct Gdi {
    edit: HFONT,
    code: HFONT,
    label: HFONT,
    card: HBRUSH,
    win: HBRUSH,
}

impl Gdi {
    fn new(typography: &Typography, dpi: u32, mode: Mode) -> Self {
        unsafe {
            Self {
                edit: make_font(typography, EDIT_FONT, dpi),
                code: make_font(typography, CODE_FONT, dpi),
                label: make_font(typography, LABEL_FONT, dpi),
                card: CreateSolidBrush(colorref(mode, Role::Card)),
                win: CreateSolidBrush(colorref(mode, Role::Win)),
            }
        }
    }
}

impl Drop for Gdi {
    fn drop(&mut self) {
        unsafe {
            for font in [self.edit, self.code, self.label] {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
            for brush in [self.card, self.win] {
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        }
    }
}

/// Everything the dialog draws with, and the state of what it draws.
pub struct Ui {
    typography: Rc<Typography>,
    factory: ID2D1Factory,
    gdi: RefCell<Gdi>,
    mode: Cell<Mode>,
    dpi: Cell<u32>,
    /// Which box has the keyboard focus.
    pub focus: Cell<Focus>,
    /// The button the pointer is over.
    pub hover: Cell<Option<Kind>>,
    /// Whether a `WM_MOUSELEAVE` is asked for.
    pub tracking: Cell<bool>,
    /// The size of the client area in DIPs that the controls are placed for.
    pub size: Cell<(f32, f32)>,
}

impl Ui {
    pub fn new(mode: Mode, dpi: u32) -> Result<Self> {
        let typography = Rc::new(Typography::new()?);
        let factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let gdi = Gdi::new(&typography, dpi, mode);
        Ok(Self {
            typography,
            factory,
            gdi: RefCell::new(gdi),
            mode: Cell::new(mode),
            dpi: Cell::new(dpi),
            focus: Cell::new(Focus::None),
            hover: Cell::new(None),
            tracking: Cell::new(false),
            size: Cell::new((0.0, 0.0)),
        })
    }

    pub fn typography(&self) -> &Typography {
        &self.typography
    }

    pub fn dpi(&self) -> u32 {
        self.dpi.get()
    }

    pub fn mode(&self) -> Mode {
        self.mode.get()
    }

    /// Changes the DPI or the mode, and makes the fonts and brushes again.
    pub fn restyle(&self, mode: Mode, dpi: u32) {
        self.mode.set(mode);
        self.dpi.set(dpi);
        *self.gdi.borrow_mut() = Gdi::new(&self.typography, dpi, mode);
    }

    pub fn edit_font(&self) -> HFONT {
        self.gdi.borrow().edit
    }

    pub fn code_font(&self) -> HFONT {
        self.gdi.borrow().code
    }

    pub fn label_font(&self) -> HFONT {
        self.gdi.borrow().label
    }

    pub fn card_brush(&self) -> HBRUSH {
        self.gdi.borrow().card
    }

    pub fn win_brush(&self) -> HBRUSH {
        self.gdi.borrow().win
    }

    /// Draws `shapes` (in DIPs) on `hdc`, which is `size` pixels, through a
    /// bitmap in memory, so that nothing flickers.
    pub fn draw(&self, hdc: HDC, size: (i32, i32), shapes: &[Shape]) -> Result<()> {
        let surface = Surface::new(size.0, size.1)?;
        let dpi = self.dpi.get() as f32;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi,
            dpiY: dpi,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        unsafe {
            let target = self.factory.CreateDCRenderTarget(&properties)?;
            target.BindDC(
                surface.hdc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: size.0,
                    bottom: size.1,
                },
            )?;
            let mut painter = Painter::new(&self.factory, target.cast()?, self.typography.clone())?;
            painter.set_mode(self.mode.get());
            painter.begin();
            painter.draw_shapes(shapes, (0.0, 0.0));
            painter.end()?;
            let _ = BitBlt(hdc, 0, 0, size.0, size.1, Some(surface.hdc), 0, 0, SRCCOPY);
        }
        Ok(())
    }

    /// The dialog: its background, and what is not a control. `address` is
    /// what the address box holds.
    pub fn draw_dialog(&self, hdc: HDC, size: (i32, i32), address: &str) -> Result<()> {
        let model = Model {
            address: address.to_owned(),
            focus: self.focus.get(),
        };
        let frame = frame(&model, &*self.typography);
        let scale = self.dpi.get() as f32 / 96.0;
        let mut shapes = vec![Shape::Rect {
            rect: Rect::new(0.0, 0.0, size.0 as f32 / scale, size.1 as f32 / scale),
            radius: 0.0,
            fill: Some(Role::Win),
            border: None,
        }];
        shapes.extend(frame.shapes);
        self.draw(hdc, size, &shapes)
    }

    /// A button of `kind`, `size` pixels, on `hdc`.
    pub fn draw_button(
        &self,
        hdc: HDC,
        size: (i32, i32),
        kind: Kind,
        state: ButtonState,
        label: &str,
    ) -> Result<()> {
        let scale = self.dpi.get() as f32 / 96.0;
        let area = Rect::new(0.0, 0.0, size.0 as f32 / scale, size.1 as f32 / scale);
        self.draw(hdc, size, &button_shapes(kind, state, label, area))
    }
}
