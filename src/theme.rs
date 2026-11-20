//! The two colour palettes, as data. Reading the system's light or dark
//! setting is the Windows layer's job.

/// An sRGB colour with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

const fn rgb(hex: u32) -> Color {
    Color {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
        a: 1.0,
    }
}

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color { r, g, b, a }
}

/// What a colour is for. Shapes name a role and the palette picks the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Win,
    WinBorder,
    Card,
    CardBorder,
    Text,
    Sub,
    Faint,
    Accent,
    Mark,
    Track,
    Pill,
    Ample,
    Low,
    Crit,
    WarnBg,
    WarnText,
    /// Hover fill of a button.
    Hover,
    /// Fill of the command box in the unavailable state.
    CommandBg,
    /// Border of the secondary button.
    ButtonBorder,
    /// One step of the shadow around the window; steps add up.
    Shadow,
    /// Fill of the primary button.
    PrimaryBg,
    /// Label of the primary button.
    PrimaryText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    /// The mode for the system's `AppsUseLightTheme` value; a missing value
    /// means light, as it does for Windows itself.
    pub fn from_apps_use_light_theme(value: Option<u32>) -> Self {
        match value {
            Some(0) => Self::Dark,
            _ => Self::Light,
        }
    }

    pub fn color(self, role: Role) -> Color {
        let light = self == Self::Light;
        match role {
            Role::Win => pick(light, rgb(0xF3F3F3), rgb(0x202020)),
            Role::WinBorder => pick(light, rgba(0, 0, 0, 0.10), rgba(255, 255, 255, 0.09)),
            Role::Card => pick(light, rgb(0xFFFFFF), rgb(0x2B2B2B)),
            Role::CardBorder => pick(light, rgb(0xE3E3E3), rgb(0x383838)),
            Role::Text => pick(light, rgb(0x1B1B1B), rgb(0xF2F2F2)),
            Role::Sub => pick(light, rgb(0x5F5F5F), rgb(0xB3B3B3)),
            Role::Faint => pick(light, rgb(0x6B6B6B), rgb(0xA0A0A0)),
            Role::Accent => pick(light, rgb(0x0E6A69), rgb(0x5CC6BF)),
            Role::Mark => pick(light, rgb(0x8A2433), rgb(0xD0606C)),
            Role::Track => pick(light, rgb(0xE1E1E1), rgb(0x3E3E3E)),
            Role::Pill => pick(light, rgb(0xEEEEEE), rgb(0x363636)),
            Role::Ample => pick(light, rgb(0x2E7D32), rgb(0x6CCB5F)),
            Role::Low => pick(light, rgb(0x8A5A00), rgb(0xF0C14B)),
            Role::Crit => pick(light, rgb(0xC42B1C), rgb(0xFF8F84)),
            Role::WarnBg => pick(light, rgb(0xFFF4CE), rgb(0x433519)),
            Role::WarnText => pick(light, rgb(0x6B4A00), rgb(0xF5D78A)),
            Role::Hover => pick(light, rgba(0, 0, 0, 0.06), rgba(255, 255, 255, 0.08)),
            Role::CommandBg => pick(light, rgb(0xF5F5F5), rgb(0x202020)),
            Role::ButtonBorder => pick(light, rgb(0xD1D1D1), rgb(0x4A4A4A)),
            Role::Shadow => pick(light, rgba(0, 0, 0, 0.022), rgba(0, 0, 0, 0.045)),
            Role::PrimaryBg => pick(light, rgb(0x0E6A69), rgb(0x5CC6BF)),
            Role::PrimaryText => pick(light, rgb(0xFFFFFF), rgb(0x0B1F1E)),
        }
    }
}

fn pick(light: bool, on_light: Color, on_dark: Color) -> Color {
    if light { on_light } else { on_dark }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_nonzero_apps_theme_value_is_light() {
        assert_eq!(Mode::from_apps_use_light_theme(None), Mode::Light);
        assert_eq!(Mode::from_apps_use_light_theme(Some(1)), Mode::Light);
        assert_eq!(Mode::from_apps_use_light_theme(Some(0)), Mode::Dark);
    }

    #[test]
    fn palette_matches_the_spec_for_both_modes() {
        assert_eq!(Mode::Light.color(Role::Win), rgb(0xF3F3F3));
        assert_eq!(Mode::Dark.color(Role::Win), rgb(0x202020));
        assert_eq!(Mode::Light.color(Role::Crit), rgb(0xC42B1C));
        assert_eq!(Mode::Dark.color(Role::Crit), rgb(0xFF8F84));
        assert_eq!(Mode::Dark.color(Role::PrimaryBg), rgb(0x5CC6BF));
        assert_eq!(Mode::Dark.color(Role::PrimaryText), rgb(0x0B1F1E));
        assert_eq!(Mode::Light.color(Role::PrimaryBg), rgb(0x0E6A69));
        assert_eq!(Mode::Dark.color(Role::WinBorder).a, 0.09);
    }
}
