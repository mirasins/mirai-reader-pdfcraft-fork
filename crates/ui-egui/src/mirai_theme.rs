//! The fork's Aura Dark colours on the framework's Graphite theme.

use std::sync::{Arc, OnceLock};

use craft::theme::{ColorToken, Rgba, ThemeDef, ThemeId};

pub(crate) fn theme() -> Option<Arc<ThemeDef>> {
    static THEME: OnceLock<Option<Arc<ThemeDef>>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            use ColorToken::*;
            let mut theme = craft::theme::builtins().iter().find(|theme| theme.id.as_str() == "craft.graphite")?.clone();
            theme.id = ThemeId::parse("mirai.reader").ok()?;
            theme.name = "Mirai Reader".into();
            // Aura Dark (Dalton Mendonça, MIT), mapped to the shared theme tokens.
            let neutral = Rgba::rgb(0x15, 0x14, 0x1b);
            let accent = Rgba::rgb(0x61, 0xff, 0xca);
            let border = Rgba::rgb(0x3b, 0x33, 0x4b);
            theme.mode_seeds.dark.neutral = Some(neutral);
            theme.mode_seeds.dark.accent = Some(accent);
            theme.pins.dark.colors.extend([
                (AccentSolid, accent),
                (LineBorder, border),
                (LineFocus, accent),
                (StateHover, border),
                (StateSelected, Rgba::rgb(0x29, 0x26, 0x3c)),
                (SurfaceBackground, Rgba::rgb(0x11, 0x0f, 0x18)),
                (SurfacePanel, neutral),
                (SurfaceRaised, Rgba::rgb(0x2e, 0x2b, 0x38)),
                (TextMuted, Rgba::rgb(0xcd, 0xcc, 0xce)),
                (TextNormal, Rgba::rgb(0xed, 0xec, 0xee)),
                (TextOnAccent, neutral),
            ]);
            Some(Arc::new(theme))
        })
        .clone()
}
