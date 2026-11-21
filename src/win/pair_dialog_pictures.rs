//! Pictures of the pairing dialog, and the tests of what makes them: one
//! file per state of the dialog in the folder that `ULLAGE_DIALOG_SHOTS`
//! names, when it names one. They are for the one who changes how the dialog
//! looks, and are not part of the program.

use std::thread;
use std::time::Duration;

use windows::Win32::Graphics::Gdi::SetTextColor;

use super::tests::{TURN, click, drive, enabled, focus};
use super::*;

/// Whether a picture of `width` pixels across has the title of the dialog
/// where it is drawn (24 DIPs from the top left of the client area, which is
/// at `offset`): something there that is not the colour of the corner. A
/// frame around the client area, or the controls, do not draw it.
fn scene_is_there(pixels: &[u8], width: i32, offset: (i32, i32), dpi: u32) -> bool {
    let scale = dpi as f32 / 96.0;
    let at = |x: i32, y: i32| -> Option<[u8; 3]> {
        let index = ((y * width + x) * 4) as usize;
        let pixel = pixels.get(index..index + 3)?;
        Some([pixel[0], pixel[1], pixel[2]])
    };
    let Some(corner) = at(offset.0 + 2, offset.1 + 2) else {
        return false;
    };
    let (left, top) = (
        offset.0 + (24.0 * scale) as i32,
        offset.1 + (24.0 * scale) as i32,
    );
    let (across, down) = ((150.0 * scale) as i32, (24.0 * scale) as i32);
    (top..top + down)
        .flat_map(|y| (left..left + across).map(move |x| (x, y)))
        .any(|(x, y)| at(x, y).is_some_and(|pixel| pixel != corner))
}

/// Saves what the dialog shows as a BMP named `name` in the folder that
/// `ULLAGE_DIALOG_SHOTS` names, when it names one. `PrintWindow` is asked
/// first, because it gives the whole window, with its title bar. When what it
/// gives is one colour (a session without a screen gives black), the window is
/// asked to print itself, which gives its client area and its controls, and
/// the text of the address box is added (see `write_address`). Only the
/// picture that is used is saved.
fn shoot(dialog: HWND, name: &str, mode: Mode) {
    // `PrintWindow` is in a part of the `windows` crate that the program
    // does not otherwise need, so it is declared here, for the tests only.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(
            hwnd: HWND,
            hdc: windows::Win32::Graphics::Gdi::HDC,
            flags: u32,
        ) -> windows::core::BOOL;
    }
    const WM_PRINT: u32 = 0x0317;
    // PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND | PRF_CHILDREN
    const PRINT_ALL: usize = 0x2 | 0x4 | 0x8 | 0x10;
    let Ok(folder) = std::env::var("ULLAGE_DIALOG_SHOTS") else {
        return;
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(dialog, &mut rect);
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    // Where the client area is in a picture of the whole window, and the DPI.
    let mut client = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(dialog, &mut client);
    }
    let offset = (client.x - rect.left, client.y - rect.top);
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(dialog) }.max(96);
    // The scene is what only the dialog itself draws (the title). A picture
    // that has a frame, or controls, and no scene is not a picture of it.
    let has_scene = |surface: &mut crate::win::surface::Surface, offset| {
        scene_is_there(surface.pixels_mut(), width, offset, dpi)
    };
    let mut surface = crate::win::surface::Surface::new(width, height).unwrap();
    unsafe {
        let _ = PrintWindow(dialog, surface.hdc, 2);
    }
    if !has_scene(&mut surface, offset) {
        surface = crate::win::surface::Surface::new(width, height).unwrap();
        unsafe {
            SendMessageW(
                dialog,
                WM_PRINT,
                Some(WPARAM(surface.hdc.0 as usize)),
                Some(LPARAM(PRINT_ALL as isize)),
            );
        }
        // The text of the address box is added to a picture of the window
        // only when the window drew itself: it is not to cover a blank one.
        if has_scene(&mut surface, offset) {
            write_address(dialog, &surface, rect, mode);
        }
    }
    // Last, when the window gave nothing: the scene of the dialog, drawn the
    // way the dialog draws it (`Ui`), with its buttons, but without the
    // native controls.
    if !has_scene(&mut surface, offset) {
        surface = crate::win::surface::Surface::new(width, height).unwrap();
        let ui = Ui::new(mode, 96).unwrap();
        ui.draw_dialog_with_buttons(
            surface.hdc,
            (width, height),
            &text_of(dialog, ID_ADDRESS),
            enabled(dialog, ID_OK),
        )
        .expect("the scene of the dialog is drawn");
    }
    // A picture of the scene alone has no frame around it: its client area is
    // at the origin.
    assert!(
        has_scene(&mut surface, offset) || has_scene(&mut surface, (0, 0)),
        "the picture of {name} has no scene"
    );
    let pixels = surface.pixels_mut();
    // The alpha of a window that is drawn by the system can be 0.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    let mut file = Vec::new();
    let size = pixels.len() as u32;
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(54 + size).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&width.to_le_bytes());
    // A negative height: the rows run top to bottom.
    file.extend_from_slice(&(-height).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&[0; 24]);
    file.extend_from_slice(pixels);
    let path = std::path::Path::new(&folder).join(format!("{name}.bmp"));
    std::fs::write(path, file).unwrap();
}

/// A single-line edit control does not answer `WM_PRINT` in a session that has
/// no screen (a multi-line one does), so the text of the address box is drawn
/// into the picture by the test, in the font and the colour of the box, where
/// the box is. This is the one part of a picture that the test draws.
fn write_address(dialog: HWND, surface: &crate::win::surface::Surface, window: RECT, mode: Mode) {
    use windows::Win32::Graphics::Gdi::{HGDIOBJ, SelectObject, SetBkMode, TRANSPARENT, TextOutW};
    const WM_GETFONT: u32 = 0x0031;
    let Ok(edit) = (unsafe { GetDlgItem(Some(dialog), ID_ADDRESS.into()) }) else {
        return;
    };
    let mut at = RECT::default();
    unsafe {
        let _ = GetWindowRect(edit, &mut at);
    }
    let font = unsafe { SendMessageW(edit, WM_GETFONT, None, None) }.0;
    let text: Vec<u16> = text_of(dialog, ID_ADDRESS).encode_utf16().collect();
    unsafe {
        let previous = SelectObject(surface.hdc, HGDIOBJ(font as *mut _));
        SetBkMode(surface.hdc, TRANSPARENT);
        SetTextColor(surface.hdc, colorref(mode, Role::Text));
        let _ = TextOutW(
            surface.hdc,
            at.left - window.left,
            at.top - window.top,
            &text,
        );
        SelectObject(surface.hdc, previous);
    }
}

/// One picture of the dialog in a state. It writes files only when
/// `ULLAGE_DIALOG_SHOTS` is set; the dialog is shown either way.
fn picture(name: &'static str, mode: Mode, address: &'static str, code: &'static str) {
    let driver = drive(move |dialog| {
        if !address.is_empty() {
            set_text(dialog, ID_ADDRESS, address);
        }
        if !code.is_empty() {
            set_text(dialog, ID_CODE, code);
        }
        focus(dialog, ID_OK);
        // The dialog draws on the thread that shows it; give it a moment.
        thread::sleep(Duration::from_millis(300));
        shoot(dialog, name, mode);
        click(dialog, ID_CANCEL);
    });
    let shown = run_with_mode(HWND::default(), None, mode);
    driver.join().unwrap();
    assert!(shown.unwrap().is_none());
}

#[test]
fn the_dialog_can_be_pictured_in_each_state() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    // The code is a made-up one.
    picture("light", Mode::Light, "100.71.140.68:7878", "abcdef");
    picture("dark", Mode::Dark, "100.71.140.68:7878", "abcdef");
    picture("invalid", Mode::Dark, "8.8.8.8:80", "abcdef");
    picture("plaintext", Mode::Light, "center:7878", "abcdef");
    picture("incomplete", Mode::Light, "", "");
}

#[test]
fn the_scene_picture_has_the_buttons_in_their_states() {
    use crate::scene::pair::{Focus, Model, frame};
    use crate::win::surface::Surface;
    for (enabled, role) in [(true, Role::PrimaryBg), (false, Role::Track)] {
        let ui = Ui::new(Mode::Light, 96).unwrap();
        let mut surface = Surface::new(420, 400).unwrap();
        ui.draw_dialog_with_buttons(surface.hdc, (420, 400), "", enabled)
            .unwrap();
        let model = Model {
            address: String::new(),
            focus: Focus::None,
        };
        let frame = frame(&model, ui.typography());
        // A point of the main button that has no label on it.
        let (x, y) = (
            (frame.primary.x + 8.0) as usize,
            (frame.primary.y + frame.primary.h / 2.0) as usize,
        );
        let pixels = surface.pixels_mut();
        let at = (y * 420 + x) * 4;
        let want = colorref(Mode::Light, role);
        assert_eq!(
            (pixels[at + 2], pixels[at + 1], pixels[at]),
            (
                (want.0 & 0xFF) as u8,
                ((want.0 >> 8) & 0xFF) as u8,
                ((want.0 >> 16) & 0xFF) as u8
            ),
            "main button, enabled: {enabled}"
        );
    }
}

#[test]
fn a_picture_with_a_frame_and_no_title_is_not_taken_for_the_dialog() {
    let (width, height) = (300usize, 120usize);
    let mut pixels = vec![200u8; width * height * 4];
    let put = |pixels: &mut Vec<u8>, x: usize, y: usize, value: u8| {
        let at = (y * width + x) * 4;
        pixels[at..at + 3].fill(value);
    };
    // A frame of another colour all round, and a control in the middle.
    for x in 0..width {
        for y in [0, 1, height - 1, height - 2] {
            put(&mut pixels, x, y, 30);
        }
    }
    for x in 100..180 {
        for y in 60..90 {
            put(&mut pixels, x, y, 90);
        }
    }
    assert!(!scene_is_there(&pixels, width as i32, (0, 0), 96));
    // The title: dark pixels where it is drawn.
    for x in 30..80 {
        put(&mut pixels, x, 34, 20);
    }
    assert!(scene_is_there(&pixels, width as i32, (0, 0), 96));
    // With the client area inside a frame of 8 pixels, the title is 8 further.
    let mut framed = vec![200u8; width * height * 4];
    for x in 38..88 {
        put(&mut framed, x, 42, 20);
    }
    assert!(scene_is_there(&framed, width as i32, (8, 8), 96));
    // Nothing to look at.
    assert!(!scene_is_there(&[], 10, (0, 0), 96));
}
