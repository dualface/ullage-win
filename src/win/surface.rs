//! A 32-bit bitmap in memory that Direct2D draws on.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::core::Result;

pub struct Surface {
    pub hdc: HDC,
    pub bitmap: HBITMAP,
    previous: HGDIOBJ,
    /// Rows run top to bottom, four bytes (B, G, R, A) a pixel.
    pub bits: *mut u8,
    pub size: (i32, i32),
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Result<Self> {
        let (width, height) = (width.max(1), height.max(1));
        unsafe {
            let hdc = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // A negative height makes the rows run top to bottom.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap =
                match CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(bitmap) => bitmap,
                    Err(error) => {
                        let _ = DeleteDC(hdc);
                        return Err(error);
                    }
                };
            let previous = SelectObject(hdc, HGDIOBJ(bitmap.0));
            Ok(Self {
                hdc,
                bitmap,
                previous,
                bits: bits.cast(),
                size: (width, height),
            })
        }
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.bits, (self.size.0 * self.size.1 * 4) as usize)
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}
