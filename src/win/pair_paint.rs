//! What the pairing dialog needs to draw itself: the fonts and brushes of its
//! controls, the Direct2D painter, and the state of the parts that are drawn
//! (the focus, the button the pointer is over). The shapes themselves are
//! `scene::pair`'s.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Factory,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CombineRgn, CreateFontW, CreateRectRgn,
    CreateSolidBrush, DEFAULT_CHARSET, DeleteObject, GetClipRgn, GetDCOrgEx, GetObjectType,
    GetRandomRgn, GetRgnBox, HBRUSH, HDC, HFONT, HGDIOBJ, HRGN, OUT_DEFAULT_PRECIS, OffsetRgn,
    RGN_COMBINE_MODE, SRCCOPY,
};
use windows::core::{Interface, Result};

use super::gfx::{Painter, make_target};
use super::surface::Surface;
use super::text::Typography;
#[cfg(test)]
use crate::scene::pair::primary_label;
use crate::scene::pair::{
    ButtonState, CODE_FONT, EDIT_FONT, Focus, Frame, Kind, LABEL_FONT, Model, Status,
    button_shapes, frame, status, to_pixels,
};
use crate::scene::{Font, Rect, Shape};
use crate::theme::{Color, Mode, Role};

/// The colour of a role as GDI wants it. Only roles without transparency are
/// used for brushes.
pub fn colorref(mode: Mode, role: Role) -> COLORREF {
    let Color { r, g, b, .. } = mode.color(role);
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// Whether `hdc` is a device context: a handle of another kind of GDI object
/// (a brush, a bitmap) or a number that is no handle is not one.
pub fn is_dc(hdc: HDC) -> bool {
    // `OBJ_DC` and `OBJ_MEMDC`.
    matches!(unsafe { GetObjectType(HGDIOBJ(hdc.0)) }, 3 | 10)
}

/// Regions that are deleted when this is dropped.
struct Regions(Vec<HRGN>);

impl Regions {
    /// A region of the rectangle, or `None` when Windows made none.
    fn rect(&mut self, left: i32, top: i32, right: i32, bottom: i32) -> Option<HRGN> {
        let region = unsafe { CreateRectRgn(left, top, right, bottom) };
        if region.is_invalid() {
            return None;
        }
        self.0.push(region);
        Some(region)
    }
}

impl Drop for Regions {
    fn drop(&mut self) {
        for region in &self.0 {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(region.0));
            }
        }
    }
}

/// Counts the calls of a check, so that a test can make any one of them fail:
/// the one at `fail_at` counts as having failed, whatever it did. The program
/// injects no failure (`fail_at` is `None`), and a call that really fails is
/// still a failure: it makes the answer no, and the note is not registered.
pub(super) struct Trip {
    fail_at: Option<usize>,
    pub(super) calls: usize,
}

impl Trip {
    pub(super) fn new(fail_at: Option<usize>) -> Self {
        Self { fail_at, calls: 0 }
    }

    /// Whether a call that `worked` is to be taken as having worked.
    fn worked(&mut self, worked: bool) -> bool {
        self.calls += 1;
        worked && self.fail_at != Some(self.calls)
    }
}

/// Whether every pixel of `rect` (x, y, width, height, in the pixels of the
/// device context) can be seen on it.
pub(super) fn fully_visible(hdc: HDC, rect: (i32, i32, i32, i32)) -> bool {
    fully_visible_by(hdc, rect, &mut Trip::new(None))
}

/// [`fully_visible`], with a [`Trip`] to make a call fail. The note is in the
/// region of the device context that the system gives (what shows of the
/// window: not off the screen, not under another window, not outside what is
/// to be painted) and in the clip region that was selected into it. A box
/// round what shows is not enough, because what shows can have a hole.
///
/// Any call that fails makes the answer no: an answer that is not known is
/// not that the note can be seen, and an empty region from a failed operation
/// is not an empty part of the note.
pub(super) fn fully_visible_by(hdc: HDC, rect: (i32, i32, i32, i32), trip: &mut Trip) -> bool {
    const RGN_OR: RGN_COMBINE_MODE = RGN_COMBINE_MODE(2);
    const RGN_DIFF: RGN_COMBINE_MODE = RGN_COMBINE_MODE(4);
    const SYSRGN: i32 = 4;
    const ERROR: i32 = 0;
    const NULLREGION: i32 = 1;
    let (x, y, w, h) = rect;
    let mut held = Regions(Vec::new());
    let mut make = |trip: &mut Trip, rect: (i32, i32, i32, i32)| {
        let region = held.rect(rect.0, rect.1, rect.2, rect.3);
        region.filter(|_| trip.worked(true))
    };
    let (Some(note), Some(uncovered), Some(scratch), Some(system), Some(clip)) = (
        make(trip, (x, y, x + w, y + h)),
        make(trip, (0, 0, 0, 0)),
        make(trip, (0, 0, 0, 0)),
        make(trip, (0, 0, 0, 0)),
        make(trip, (0, 0, 0, 0)),
    ) else {
        return false;
    };
    unsafe {
        // What the system gives is in the coordinates of the screen.
        let mut origin = POINT::default();
        if !trip.worked(GetDCOrgEx(hdc, &mut origin).as_bool()) {
            return false;
        }
        if !trip.worked(GetRandomRgn(hdc, system, SYSRGN) == 1) {
            return false;
        }
        if !trip.worked(OffsetRgn(system, -origin.x, -origin.y).0 != ERROR) {
            return false;
        }
        // What of the note is not in it.
        let left_out = CombineRgn(Some(uncovered), Some(note), Some(system), RGN_DIFF);
        if !trip.worked(left_out.0 != ERROR) {
            return false;
        }
        // And what was selected as the clip region: 0 is that there is none,
        // and -1 is that Windows could not say.
        let selected = GetClipRgn(hdc, clip);
        if !trip.worked(selected >= 0) {
            return false;
        }
        if selected == 1 {
            let cut = CombineRgn(Some(scratch), Some(note), Some(clip), RGN_DIFF);
            if !trip.worked(cut.0 != ERROR) {
                return false;
            }
            let joined = CombineRgn(Some(uncovered), Some(uncovered), Some(scratch), RGN_OR);
            if !trip.worked(joined.0 != ERROR) {
                return false;
            }
        }
        let mut bounds = RECT::default();
        let kind = GetRgnBox(uncovered, &mut bounds).0;
        trip.worked(kind != ERROR) && kind == NULLREGION
    }
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
pub struct Gdi {
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
    /// The address for which the plain text note has been drawn, so that it
    /// has been on the screen before that address can be paired with.
    pub noted: RefCell<Option<String>>,
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
            noted: RefCell::new(None),
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
    /// The old ones are given back, not deleted: the controls still hold the
    /// old fonts until they are given the new ones, and the caller lets go of
    /// the old ones after that.
    #[must_use = "the old fonts are deleted when this is dropped, after the controls have new ones"]
    pub fn restyle(&self, mode: Mode, dpi: u32) -> Gdi {
        self.mode.set(mode);
        self.dpi.set(dpi);
        std::mem::replace(
            &mut *self.gdi.borrow_mut(),
            Gdi::new(&self.typography, dpi, mode),
        )
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
        self.draw_layers(hdc, size, &[(shapes, (0.0, 0.0))])
    }

    /// Draws each layer of shapes at its own origin (in DIPs).
    pub fn draw_layers(
        &self,
        hdc: HDC,
        size: (i32, i32),
        layers: &[(&[Shape], (f32, f32))],
    ) -> Result<()> {
        let surface = Surface::new(size.0, size.1)?;
        unsafe {
            let target = make_target(&self.factory, &surface, self.dpi.get())?;
            let mut painter = Painter::new(&self.factory, target.cast()?, self.typography.clone())?;
            painter.set_mode(self.mode.get());
            painter.begin();
            for (shapes, origin) in layers {
                painter.draw_shapes(shapes, *origin);
            }
            painter.end()?;
            BitBlt(hdc, 0, 0, size.0, size.1, Some(surface.hdc), 0, 0, SRCCOPY)?;
        }
        Ok(())
    }

    /// The dialog: its background, and what is not a control. `address` is
    /// what the address box holds.
    ///
    /// `on_screen` is for a device context that is the window's own, the one
    /// that `WM_PAINT` gives: only what is drawn there is on the screen, and
    /// only then is the note of a path that is not encrypted taken to have
    /// been seen. A drawing that goes to memory (a print, a picture) is not.
    pub fn draw_dialog(
        &self,
        hdc: HDC,
        size: (i32, i32),
        address: &str,
        on_screen: bool,
    ) -> Result<()> {
        let (frame, shapes) = self.dialog_shapes(size, address);
        self.draw(hdc, size, &shapes)?;
        // The note is on the screen now, wholly: that address may be paired
        // with. A copy that went through but left the note, or a part of it,
        // out is not enough.
        if on_screen
            && status(address) == Status::Plaintext
            && frame
                .note
                .is_some_and(|note| fully_visible(hdc, to_pixels(note, self.dpi.get())))
        {
            *self.noted.borrow_mut() = Some(address.to_owned());
        }
        Ok(())
    }

    /// The layout and the shapes of the dialog for `address`, on a background
    /// of `size` pixels.
    fn dialog_shapes(&self, size: (i32, i32), address: &str) -> (Frame, Vec<Shape>) {
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
        shapes.extend(frame.shapes.iter().cloned());
        (frame, shapes)
    }

    /// The dialog as `draw_dialog` draws it, with its two buttons too, the
    /// main one in the state that `primary_enabled` says. For a picture that
    /// is made without the controls of the window.
    #[cfg(test)]
    pub fn draw_dialog_with_buttons(
        &self,
        hdc: HDC,
        size: (i32, i32),
        address: &str,
        primary_enabled: bool,
    ) -> Result<()> {
        let (frame, dialog) = self.dialog_shapes(size, address);
        let label = primary_label(&status(address));
        let primary = button_shapes(
            Kind::Primary,
            ButtonState {
                disabled: !primary_enabled,
                ..Default::default()
            },
            label,
            Rect::new(0.0, 0.0, frame.primary.w, frame.primary.h),
        );
        let cancel = button_shapes(
            Kind::Secondary,
            ButtonState::default(),
            "Cancel",
            Rect::new(0.0, 0.0, frame.cancel.w, frame.cancel.h),
        );
        self.draw_layers(
            hdc,
            size,
            &[
                (&dialog, (0.0, 0.0)),
                (&primary, (frame.primary.x, frame.primary.y)),
                (&cancel, (frame.cancel.x, frame.cancel.y)),
            ],
        )
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
