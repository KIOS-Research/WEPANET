//! Fonts and the overall look of widgets and windows.
//!
//! EPANET-UI uses the system UI font (Segoe UI on Windows). On Windows we load
//! it when present; elsewhere, and in the browser, we use the embedded
//! Selawik, Microsoft's open-source (OFL) metric-compatible stand-in for
//! Segoe UI. A "bold" family is registered for menu tabs and titles.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke};

pub const BOLD: &str = "bold";

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

static SELAWIK: &[u8] = include_bytes!("../assets/fonts/Selawik-Regular.ttf");
static SELAWIK_BOLD: &[u8] = include_bytes!("../assets/fonts/Selawik-Bold.ttf");
/// Courier Prime (OFL), a Courier New look-alike with plain zeros, for the
/// tables and text reports where EPANET-UI uses Courier New.
static COURIER_PRIME: &[u8] = include_bytes!("../assets/fonts/CourierPrime-Regular.ttf");
/// Noto Sans SC (OFL), cut down to the characters of the Chinese translation
/// (`lang/zh.json`), as the fallback for Chinese text. After changing the
/// translation, make it again from the full font:
/// `pyftsubset NotoSansSC-Regular.otf --text-file=<zh.json values>
/// --layout-features='*' --output-file=NotoSansSC-subset.otf`.
static NOTO_SANS_SC: &[u8] = include_bytes!("../assets/fonts/NotoSansSC-subset.otf");

/// Reads Courier New from the Windows fonts folder.
fn system_mono() -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        std::fs::read(r"C:\Windows\Fonts\cour.ttf").ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Reads Segoe UI (regular, bold) from the Windows fonts folder.
fn system_fonts() -> Option<(Vec<u8>, Vec<u8>)> {
    #[cfg(target_os = "windows")]
    {
        let dir = std::path::Path::new(r"C:\Windows\Fonts");
        let r = std::fs::read(dir.join("segoeui.ttf")).ok()?;
        let b = std::fs::read(dir.join("segoeuib.ttf")).ok()?;
        Some((r, b))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let (regular, bold_data) = match system_fonts() {
        Some((r, b)) => (FontData::from_owned(r), FontData::from_owned(b)),
        None => (
            FontData::from_static(SELAWIK),
            FontData::from_static(SELAWIK_BOLD),
        ),
    };
    fonts.font_data.insert("ui".into(), Arc::new(regular));
    fonts
        .font_data
        .insert("ui-bold".into(), Arc::new(bold_data));
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "ui".into());
    // Chinese characters, which the UI fonts do not have
    fonts
        .font_data
        .insert("cjk".into(), Arc::new(FontData::from_static(NOTO_SANS_SC)));
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.push("cjk".into());
    // Bold falls back to the regular chain (keeps symbol coverage).
    let mut bold_chain = vec!["ui-bold".to_string()];
    bold_chain.extend(proportional.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(BOLD.into()), bold_chain);
    // Monospace text (tables, reports) in Courier, as in EPANET-UI; egui's
    // default monospace font draws zeros with a slash.
    let mono = match system_mono() {
        Some(m) => FontData::from_owned(m),
        None => FontData::from_static(COURIER_PRIME),
    };
    fonts.font_data.insert("mono".into(), Arc::new(mono));
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "mono".into());
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        use egui::TextStyle::*;
        // EPANET-UI uses 9 pt text.
        for (ts, size) in [
            (Body, 12.5),
            (Button, 12.5),
            (Small, 10.0),
            (Monospace, 11.5),
            (Heading, 16.0),
        ] {
            if let Some(f) = style.text_styles.get_mut(&ts) {
                f.size = size;
            }
        }
        let v = &mut style.visuals;
        v.window_corner_radius = egui::CornerRadius::same(8);
        v.menu_corner_radius = egui::CornerRadius::same(4);
        if !v.dark_mode {
            // Black text and Windows 11-like windows and buttons.
            let black = Color32::from_gray(10);
            v.widgets.noninteractive.fg_stroke.color = black;
            v.widgets.inactive.fg_stroke.color = black;
            v.widgets.hovered.fg_stroke.color = black;
            v.widgets.active.fg_stroke.color = black;
            v.window_fill = Color32::WHITE;
            v.window_stroke = Stroke::new(1.0, Color32::from_gray(170));
            v.panel_fill = Color32::from_rgb(0xf3, 0xf3, 0xf3);
            let edge = Color32::from_gray(0xd0);
            v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xfd, 0xfd, 0xfd);
            v.widgets.inactive.bg_fill = Color32::from_rgb(0xfd, 0xfd, 0xfd);
            v.widgets.inactive.bg_stroke = Stroke::new(1.0, edge);
            v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe5, 0xf1, 0xfb);
            v.widgets.hovered.bg_fill = Color32::from_rgb(0xe5, 0xf1, 0xfb);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x78, 0xd4));
            v.widgets.active.weak_bg_fill = Color32::from_rgb(0xcc, 0xe4, 0xf7);
            v.widgets.active.bg_fill = Color32::from_rgb(0xcc, 0xe4, 0xf7);
            v.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x5a, 0x9e));
            v.selection.bg_fill = Color32::from_rgb(0xcc, 0xe4, 0xf7);
            v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x5a, 0x9e));
        } else {
            // Bright text on the dark panels.
            v.widgets.noninteractive.fg_stroke.color = Color32::from_gray(228);
            v.widgets.inactive.fg_stroke.color = Color32::from_gray(236);
            v.widgets.hovered.fg_stroke.color = Color32::from_gray(250);
            v.widgets.active.fg_stroke.color = Color32::WHITE;
            v.weak_text_color = Some(Color32::from_gray(175));
        }
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.corner_radius = egui::CornerRadius::same(3);
            w.expansion = 0.0;
        }
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
    });
}

#[cfg(test)]
mod tests {
    /// The dark theme's text is bright, its weak text still readable.
    #[test]
    fn dark_theme_text_is_bright() {
        let ctx = egui::Context::default();
        super::install(&ctx);
        let v = ctx.style_of(egui::Theme::Dark).visuals.clone();
        assert!(v.text_color().r() >= 220, "{:?}", v.text_color());
        assert!(v.weak_text_color().r() >= 160);
        let light = ctx.style_of(egui::Theme::Light).visuals.clone();
        assert!(light.text_color().r() <= 20);
    }
}
