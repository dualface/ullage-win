//! Text measuring and text formats, with DirectWrite.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
};
use windows::core::{Result, w};

use super::util::{pcwstr, wide};
use crate::scene::{Align, Font, FontKind, Measure};

const UI_FAMILIES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];
const MONO_FAMILIES: [&str; 2] = ["Cascadia Mono", "Consolas"];

#[derive(Clone, PartialEq, Eq, Hash)]
struct FormatKey {
    kind: FontKind,
    size: u32,
    weight: u16,
    align: u8,
    ellipsis: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct WidthKey {
    text: String,
    kind: FontKind,
    size: u32,
    weight: u16,
}

pub struct Typography {
    factory: IDWriteFactory,
    ui: Vec<u16>,
    mono: Vec<u16>,
    formats: RefCell<HashMap<FormatKey, IDWriteTextFormat>>,
    widths: RefCell<HashMap<WidthKey, f32>>,
}

/// The first of `names` that this system has installed.
fn first_installed(collection: &IDWriteFontCollection, names: &[&str]) -> Vec<u16> {
    for name in names {
        let wide_name = wide(name);
        let mut index = 0;
        let mut exists = false.into();
        let found =
            unsafe { collection.FindFamilyName(pcwstr(&wide_name), &mut index, &mut exists) };
        if found.is_ok() && exists.as_bool() {
            return wide_name;
        }
    }
    wide(names[names.len() - 1])
}

impl Typography {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut collection = None;
            factory.GetSystemFontCollection(&mut collection, false)?;
            let collection = collection.ok_or_else(windows::core::Error::empty)?;
            Ok(Self {
                ui: first_installed(&collection, &UI_FAMILIES),
                mono: first_installed(&collection, &MONO_FAMILIES),
                factory,
                formats: RefCell::default(),
                widths: RefCell::default(),
            })
        }
    }

    /// The name of the family that `kind` is drawn in, with its closing NUL.
    pub fn family_name(&self, kind: FontKind) -> &[u16] {
        self.family(kind)
    }

    fn family(&self, kind: FontKind) -> &[u16] {
        match kind {
            FontKind::Ui => &self.ui,
            FontKind::Mono => &self.mono,
        }
    }

    fn create_format(&self, font: Font) -> Result<IDWriteTextFormat> {
        unsafe {
            self.factory.CreateTextFormat(
                pcwstr(self.family(font.kind)),
                None,
                DWRITE_FONT_WEIGHT(i32::from(font.weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font.size,
                w!("en-us"),
            )
        }
    }

    /// A single-line format that centres its text vertically.
    pub fn format(&self, font: Font, align: Align, ellipsis: bool) -> Result<IDWriteTextFormat> {
        let key = FormatKey {
            kind: font.kind,
            size: font.size.to_bits(),
            weight: font.weight,
            align: align as u8,
            ellipsis,
        };
        if let Some(format) = self.formats.borrow().get(&key) {
            return Ok(format.clone());
        }
        let format = self.create_format(font)?;
        unsafe {
            format.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            })?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            if ellipsis {
                let sign = self.factory.CreateEllipsisTrimmingSign(&format)?;
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                format.SetTrimming(&trimming, &sign)?;
            }
        }
        self.formats.borrow_mut().insert(key, format.clone());
        Ok(format)
    }

    fn measure(&self, text: &str, font: Font) -> Result<f32> {
        let format = self.create_format(font)?;
        let chars: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let layout =
                self.factory
                    .CreateTextLayout(&chars, &format, f32::MAX / 4.0, font.size * 4.0)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(metrics.widthIncludingTrailingWhitespace)
        }
    }
}

impl Measure for Typography {
    fn width(&self, text: &str, font: Font) -> f32 {
        let key = WidthKey {
            text: text.to_owned(),
            kind: font.kind,
            size: font.size.to_bits(),
            weight: font.weight,
        };
        if let Some(width) = self.widths.borrow().get(&key) {
            return *width;
        }
        // A font that cannot be measured counts as half its size per
        // character, so the layout still works.
        let width = self
            .measure(text, font)
            .unwrap_or_else(|_| text.chars().count() as f32 * font.size * 0.5);
        let mut widths = self.widths.borrow_mut();
        if widths.len() > 4096 {
            widths.clear();
        }
        widths.insert(key, width);
        width
    }
}
