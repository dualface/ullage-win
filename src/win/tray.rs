//! The notification area icon.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, ID2D1Factory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, GdiFlush, HGDIOBJ};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON, WM_APP,
};
use windows::core::{Interface, Result};

use std::rc::Rc;

use super::gfx::Painter;
use super::surface::Surface;
use super::text::Typography;
use crate::scene::{Icon, Rect};
use crate::theme::{Mode, Role};

/// Sent to the window for mouse events on the icon.
pub const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;
const TIP: &str = "Ullage";

fn data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip(TIP.encode_utf16()) {
        *slot = unit;
    }
    data
}

pub fn add(hwnd: HWND, icon: HICON) -> bool {
    unsafe { Shell_NotifyIconW(NIM_ADD, &data(hwnd, icon)).as_bool() }
}

pub fn update(hwnd: HWND, icon: HICON) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, icon));
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd, HICON::default()));
    }
}

/// Draws the container icon at the size of a small icon, in the colours of
/// the taskbar, and makes an icon of it.
pub fn make_icon(factory: &ID2D1Factory, text: Rc<Typography>, mode: Mode) -> Result<HICON> {
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        let mut surface = Surface::new(size, size)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = factory.CreateDCRenderTarget(&properties)?;
        target.BindDC(
            surface.hdc,
            &RECT {
                left: 0,
                top: 0,
                right: size,
                bottom: size,
            },
        )?;
        let mut painter = Painter::new(factory, target.cast()?, text)?;
        painter.set_mode(mode);
        painter.begin();
        painter.clear_transparent();
        painter.draw_shapes(
            &[crate::scene::Shape::Icon {
                icon: Icon::Container { liquid: true },
                rect: Rect::new(0.0, 0.0, size as f32, size as f32),
                color: Role::Text,
            }],
            (0.0, 0.0),
        );
        painter.end()?;
        let _ = GdiFlush();

        // Direct2D wrote premultiplied pixels; an icon wants straight alpha.
        for pixel in surface.pixels_mut().chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            if alpha != 0 && alpha != 255 {
                for channel in &mut pixel[..3] {
                    *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
        }
        let mask = CreateBitmap(size, size, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: surface.bitmap,
        });
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon
    }
}
