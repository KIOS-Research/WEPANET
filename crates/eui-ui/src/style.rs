//! Colors, map themes and legends, following EPANET-UI (`config.pas`,
//! `mapthemes.pas`).

use egui::Color32;
use eui_core::{LinkExtra, LinkVar, NodeExtra, NodeVar};

/// Menu bar / panel header gray (`GrayMenuColor`).
pub const MENU_GRAY: Color32 = Color32::from_rgb(0x69, 0x79, 0x7e);
/// Menu bar / panel header blue (`BlueMenuColor`).
pub const MENU_BLUE: Color32 = Color32::from_rgb(0x10, 0x6e, 0xbe);
/// Hint / legend background (`HintColor`).
pub const HINT_BG: Color32 = Color32::from_rgb(0xff, 0xff, 0xe1);
pub const RESULTS_CURRENT: Color32 = Color32::from_rgb(0xb5, 0xe6, 0x1d);
pub const RESULTS_STALE: Color32 = Color32::from_rgb(0xff, 0xc9, 0x0e);
pub const SELECTED: Color32 = Color32::from_rgb(0xe8, 0x1e, 0x63);
pub const ACCENT: Color32 = Color32::from_rgb(0x10, 0x6e, 0xbe);

/// EPANET-UI's default legend colors (`DefLegendColors`).
pub const RAMP: [Color32; 5] = [
    Color32::from_rgb(0x70, 0x92, 0xbe),
    Color32::from_rgb(0x99, 0xd9, 0xea),
    Color32::from_rgb(0xb5, 0xe6, 0x1d),
    Color32::from_rgb(0xff, 0xc9, 0x0e),
    Color32::from_rgb(0xff, 0x7f, 0x27),
];

pub struct Palette {
    pub menu: Color32,
    pub form: Color32,
    pub map_bg: Color32,
    pub link: Color32,
    pub node: Color32,
    pub label: Color32,
}

pub fn palette(dark: bool, blue: bool) -> Palette {
    let menu = if blue { MENU_BLUE } else { MENU_GRAY };
    if dark {
        Palette {
            menu,
            form: Color32::from_rgb(0x25, 0x28, 0x2c),
            map_bg: Color32::from_rgb(0x1b, 0x1e, 0x23),
            link: Color32::from_rgb(0x9a, 0xa4, 0xb1),
            node: Color32::from_rgb(0xc9, 0xd1, 0xd9),
            label: Color32::from_rgb(0xe6, 0xea, 0xee),
        }
    } else {
        Palette {
            menu,
            form: if blue {
                Color32::from_rgb(0xe3, 0xee, 0xfd)
            } else {
                Color32::from_rgb(0xf3, 0xf3, 0xf3)
            },
            map_bg: Color32::WHITE,
            link: Color32::from_rgb(0x40, 0x40, 0x40),
            node: Color32::from_rgb(0x40, 0x40, 0x40),
            label: Color32::from_rgb(0x20, 0x20, 0x20),
        }
    }
}

/// Map background colors (Map Display Settings > Background).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MapBackground {
    #[default]
    White,
    Cream,
    Yellow,
    Green,
    Cyan,
    Gray,
    Blue,
    Black,
}

impl MapBackground {
    pub const ALL: [MapBackground; 8] = [
        MapBackground::White,
        MapBackground::Cream,
        MapBackground::Yellow,
        MapBackground::Green,
        MapBackground::Cyan,
        MapBackground::Gray,
        MapBackground::Blue,
        MapBackground::Black,
    ];
    pub fn label(self) -> &'static str {
        match self {
            MapBackground::White => "White",
            MapBackground::Cream => "Cream",
            MapBackground::Yellow => "Yellow",
            MapBackground::Green => "Green",
            MapBackground::Cyan => "Cyan",
            MapBackground::Gray => "Gray",
            MapBackground::Blue => "Blue",
            MapBackground::Black => "Black",
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            MapBackground::White => Color32::WHITE,
            MapBackground::Cream => Color32::from_rgb(0xff, 0xfb, 0xf0),
            MapBackground::Yellow => Color32::from_rgb(0xff, 0xff, 0xe1),
            MapBackground::Green => Color32::from_rgb(0xe1, 0xfc, 0xec),
            MapBackground::Cyan => Color32::from_rgb(0xf0, 0xff, 0xff),
            MapBackground::Gray => Color32::from_rgb(0xf5, 0xf5, 0xf5),
            MapBackground::Blue => Color32::from_rgb(0xe1, 0xec, 0xfc),
            MapBackground::Black => Color32::from_rgb(0x40, 0x44, 0x4b),
        }
    }
}

/// The map colors for a background: the palette's, with the map background
/// replaced (White keeps the theme's own, so dark mode stays dark) and the
/// symbol and label colors switched to suit a light or dark background.
pub fn map_palette(dark: bool, blue: bool, bg: MapBackground) -> Palette {
    let mut pal = palette(dark, blue);
    if bg == MapBackground::White {
        return pal;
    }
    let bg_dark = bg == MapBackground::Black;
    if bg_dark != dark {
        let other = palette(bg_dark, blue);
        pal.link = other.link;
        pal.node = other.node;
        pal.label = other.label;
    }
    pal.map_bg = bg.color();
    pal
}

// ----- themes -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeTheme {
    None,
    Elevation,
    BaseDemand,
    Demand,
    DemandDeficit,
    EmitterFlow,
    Leakage,
    Head,
    Pressure,
    Quality,
    /// Species `n` of the EPANET-MSX results.
    Msx(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LinkTheme {
    None,
    Diameter,
    Length,
    Roughness,
    Flow,
    Velocity,
    Headloss,
    Leakage,
    Quality,
    /// Species `n` of the EPANET-MSX results.
    Msx(u8),
}

impl NodeTheme {
    pub const ALL: [NodeTheme; 10] = [
        NodeTheme::None,
        NodeTheme::Elevation,
        NodeTheme::BaseDemand,
        NodeTheme::Demand,
        NodeTheme::DemandDeficit,
        NodeTheme::EmitterFlow,
        NodeTheme::Leakage,
        NodeTheme::Head,
        NodeTheme::Pressure,
        NodeTheme::Quality,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NodeTheme::None => "None",
            NodeTheme::Elevation => "Elevation",
            NodeTheme::BaseDemand => "Base Demand",
            NodeTheme::Demand => "Total Demand",
            NodeTheme::DemandDeficit => "Demand Deficit",
            NodeTheme::EmitterFlow => "Emitter Flow",
            NodeTheme::Leakage => "Leakage",
            NodeTheme::Head => "Hydraulic Head",
            NodeTheme::Pressure => "Pressure",
            NodeTheme::Quality => "Quality",
            NodeTheme::Msx(_) => "MSX Species",
        }
    }
    pub fn result_var(self) -> Option<NodeVar> {
        match self {
            NodeTheme::Demand => Some(NodeVar::Demand),
            NodeTheme::Head => Some(NodeVar::Head),
            NodeTheme::Pressure => Some(NodeVar::Pressure),
            NodeTheme::Quality => Some(NodeVar::Quality),
            _ => None,
        }
    }
    /// EPANET-UI's default legend intervals.
    pub fn default_breaks(self) -> [f32; 4] {
        match self {
            NodeTheme::Quality => [1.0, 10.0, 50.0, 80.0],
            NodeTheme::EmitterFlow | NodeTheme::Leakage => [5.0, 10.0, 20.0, 50.0],
            _ => [25.0, 50.0, 75.0, 100.0],
        }
    }
    /// For results the engine saves besides the output file.
    pub fn extra(self) -> Option<NodeExtra> {
        match self {
            NodeTheme::DemandDeficit => Some(NodeExtra::DemandDeficit),
            NodeTheme::EmitterFlow => Some(NodeExtra::EmitterFlow),
            NodeTheme::Leakage => Some(NodeExtra::Leakage),
            _ => None,
        }
    }
    /// Whether the theme shows simulation results.
    pub fn is_result(self) -> bool {
        matches!(self, NodeTheme::Msx(_)) || self.result_var().is_some() || self.extra().is_some()
    }
}

impl LinkTheme {
    pub const ALL: [LinkTheme; 9] = [
        LinkTheme::None,
        LinkTheme::Diameter,
        LinkTheme::Length,
        LinkTheme::Roughness,
        LinkTheme::Flow,
        LinkTheme::Velocity,
        LinkTheme::Headloss,
        LinkTheme::Leakage,
        LinkTheme::Quality,
    ];
    pub fn label(self) -> &'static str {
        match self {
            LinkTheme::None => "None",
            LinkTheme::Diameter => "Diameter",
            LinkTheme::Length => "Length",
            LinkTheme::Roughness => "Roughness",
            LinkTheme::Flow => "Flow Rate",
            LinkTheme::Velocity => "Velocity",
            LinkTheme::Headloss => "Head Loss",
            LinkTheme::Leakage => "Leakage",
            LinkTheme::Quality => "Quality",
            LinkTheme::Msx(_) => "MSX Species",
        }
    }
    pub fn result_var(self) -> Option<LinkVar> {
        match self {
            LinkTheme::Flow => Some(LinkVar::Flow),
            LinkTheme::Velocity => Some(LinkVar::Velocity),
            LinkTheme::Headloss => Some(LinkVar::Headloss),
            LinkTheme::Quality => Some(LinkVar::Quality),
            _ => None,
        }
    }
    pub fn default_breaks(self) -> [f32; 4] {
        match self {
            LinkTheme::Diameter => [6.0, 12.0, 24.0, 36.0],
            LinkTheme::Length => [100.0, 500.0, 1000.0, 5000.0],
            LinkTheme::Roughness => [50.0, 75.0, 100.0, 125.0],
            LinkTheme::Velocity => [0.01, 0.1, 1.0, 2.0],
            LinkTheme::Headloss => [0.025, 0.05, 0.075, 0.1],
            LinkTheme::Quality => [1.0, 10.0, 50.0, 80.0],
            LinkTheme::Leakage => [5.0, 10.0, 20.0, 50.0],
            _ => [25.0, 50.0, 75.0, 100.0],
        }
    }
    /// For results the engine saves besides the output file.
    pub fn extra(self) -> Option<LinkExtra> {
        match self {
            LinkTheme::Leakage => Some(LinkExtra::Leakage),
            _ => None,
        }
    }
    /// Whether the theme shows simulation results.
    pub fn is_result(self) -> bool {
        matches!(self, LinkTheme::Msx(_)) || self.result_var().is_some() || self.extra().is_some()
    }
}

/// Class breaks for a theme: 4 interval boundaries giving 5 colors. The
/// breaks are kept per theme; the colors are shared by all node themes, and
/// by all link themes, as in EPANET-UI (they are filled in from the
/// preferences when the legend is looked up).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Legend {
    pub breaks: [f32; 4],
    #[serde(skip, default = "default_colors")]
    pub colors: [Color32; 5],
}

fn default_colors() -> [Color32; 5] {
    RAMP
}

impl Legend {
    pub fn new(breaks: [f32; 4]) -> Legend {
        Legend {
            breaks,
            colors: RAMP,
        }
    }

    /// The color class (0 to 4) that `value` falls in.
    pub fn class(&self, value: f32) -> usize {
        let v = value.abs();
        self.breaks.iter().position(|&b| v < b).unwrap_or(4)
    }

    pub fn color(&self, value: f32) -> Color32 {
        self.colors[self.class(value)]
    }
}

/// EPANET-UI's legend color palettes (`themepalette.pas`), lowest class first.
pub const PALETTES: [(&str, [Color32; 5]); 12] = {
    const fn c(r: u8, g: u8, b: u8) -> Color32 {
        Color32::from_rgb(r, g, b)
    }
    [
        ("Default", RAMP),
        (
            "Rainbow",
            [
                c(0, 0, 255),
                c(0, 255, 255),
                c(0, 255, 0),
                c(255, 255, 0),
                c(255, 0, 0),
            ],
        ),
        (
            "Muted",
            [
                c(0x94, 0xcb, 0xce),
                c(0xdc, 0xcd, 0x7d),
                c(0xc2, 0x6a, 0x77),
                c(0x9f, 0x4a, 0x96),
                c(0x7e, 0x29, 0x54),
            ],
        ),
        (
            "Android",
            [
                c(0x33, 0xb5, 0xe5),
                c(0xaa, 0x66, 0xcc),
                c(0x99, 0xcc, 0x00),
                c(0xff, 0xbb, 0x33),
                c(0xff, 0x44, 0x44),
            ],
        ),
        (
            "Opera",
            [
                c(0xa3, 0x66, 0x44),
                c(0xa8, 0x97, 0x4e),
                c(0x35, 0x9c, 0xf3),
                c(0x14, 0x4c, 0xf1),
                c(0x0e, 0x08, 0xb3),
            ],
        ),
        (
            "Red",
            [
                c(0xff, 0xcc, 0xcc),
                c(0xff, 0x80, 0x80),
                c(0xff, 0, 0),
                c(0x99, 0, 0),
                c(0x66, 0, 0),
            ],
        ),
        (
            "Orange",
            [
                c(0xff, 0xe4, 0xce),
                c(0xff, 0xcc, 0x99),
                c(0xff, 0x99, 0x33),
                c(0xcc, 0x66, 0),
                c(0x99, 0x33, 0),
            ],
        ),
        (
            "Green",
            [
                c(0xce, 0xff, 0xce),
                c(0x99, 0xff, 0x99),
                c(0, 0xff, 0),
                c(0, 0x99, 0),
                c(0, 0x66, 0),
            ],
        ),
        (
            "Blue",
            [
                c(0xf9, 0xf9, 0xf9),
                c(0xd1, 0xe5, 0xf0),
                c(0x8e, 0xc4, 0xde),
                c(0x3a, 0x93, 0xc3),
                c(0x10, 0x65, 0xab),
            ],
        ),
        (
            "Purple",
            [
                c(0xff, 0xcc, 0xff),
                c(0xff, 0x99, 0xff),
                c(0xcc, 0, 0xff),
                c(0x99, 0, 0xcc),
                c(0x66, 0, 0x66),
            ],
        ),
        (
            "Magenta",
            [
                c(0xff, 0xd9, 0xec),
                c(0xff, 0x99, 0xcc),
                c(0xff, 0x33, 0xcc),
                c(0xcc, 0, 0x99),
                c(0x99, 0, 0x66),
            ],
        ),
        (
            "Gray",
            [
                c(0xf8, 0xf8, 0xf8),
                c(0xc0, 0xc0, 0xc0),
                c(0x80, 0x80, 0x80),
                c(0x42, 0x42, 0x42),
                c(0, 0, 0),
            ],
        ),
    ]
};

/// A "nice" scale over `[zmin, zmax]`: the rounded ends and a step
/// (EPANET-UI's `utils.AutoScale`).
pub fn auto_scale(zmin: f64, zmax: f64) -> (f64, f64, f64) {
    let (z1, z2) = (zmin, zmax);
    let (mut zmin, mut zmax) = (zmin, zmax);
    let d = (zmax - zmin).abs();
    if d == 0.0 && zmin == 0.0 {
        return (-1.0, 1.0, 1.0);
    } else if d < 0.01 {
        zmin -= 0.5 * zmin.abs();
        zmax += 0.5 * zmax.abs();
    }
    let d = (zmax - zmin).abs();
    if d.is_nan() || d <= 0.0 || d.is_infinite() {
        return (z1, z2, z2 - z1);
    }
    let m = (d.ln() / 10f64.ln()).trunc() as i32;
    let mut t = 10f64.powi(m);
    if t > 0.5 * d {
        t *= 0.2;
    } else if t > 0.2 * d {
        t *= 0.5;
    }
    zmax = ((zmax / t).trunc() + 1.0) * t;
    let mut z = (zmin / t).trunc();
    if zmin < 0.0 {
        z -= 1.0;
    }
    zmin = z * t;
    if zmin == zmax {
        zmax = zmin + t;
    }
    if (zmin - zmax).abs() / t > 10.0 {
        t *= 2.0;
    }
    (zmin, zmax, t)
}

/// Legend intervals from a value range, as EPANET-UI's Interval Scale does.
pub fn interval_scale(vmin: f64, vmax: f64) -> [f32; 4] {
    let (zmin, _, t) = auto_scale(vmin, vmax);
    [1.0, 2.0, 3.0, 4.0].map(|k| (zmin + k * t) as f32)
}

/// Decimal places for report values (Preferences > Numerical Precision).
static DECIMALS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(2);

pub fn set_decimals(n: usize) {
    DECIMALS.store(n.min(6), std::sync::atomic::Ordering::Relaxed);
}

/// How decimal numbers are written (Preferences > Decimal Separator).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecimalSeparator {
    /// As the computer's regional settings write them.
    #[default]
    System,
    Point,
    Comma,
}

impl DecimalSeparator {
    pub const ALL: [DecimalSeparator; 3] = [Self::System, Self::Point, Self::Comma];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Regional Settings",
            Self::Point => "Point (0.5)",
            Self::Comma => "Comma (0,5)",
        }
    }

    pub fn is_comma(self) -> bool {
        match self {
            Self::System => system_decimal_comma(),
            Self::Point => false,
            Self::Comma => true,
        }
    }
}

/// Whether the computer's (or the browser's) regional settings write
/// decimals with a comma.
fn system_decimal_comma() -> bool {
    static COMMA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *COMMA.get_or_init(|| system_decimal_separator() == Some(','))
}

#[cfg(windows)]
fn system_decimal_separator() -> Option<char> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocaleInfoEx(name: *const u16, kind: u32, data: *mut u16, len: i32) -> i32;
    }
    const LOCALE_SDECIMAL: u32 = 0x0E;
    let mut buf = [0u16; 8];
    // a null locale name is the user's default locale
    let n = unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_SDECIMAL, buf.as_mut_ptr(), 8) };
    char::decode_utf16(buf[..(n.max(1) - 1) as usize].iter().copied())
        .next()?
        .ok()
}

#[cfg(target_arch = "wasm32")]
fn system_decimal_separator() -> Option<char> {
    // how the browser's language writes 1.5
    let lang = js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .and_then(|n| js_sys::Reflect::get(&n, &"language".into()))
        .ok()?
        .as_string()?;
    let s: String = js_sys::Number::from(1.5).to_locale_string(&lang).into();
    s.chars().find(|c| !c.is_ascii_digit())
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
fn system_decimal_separator() -> Option<char> {
    // the numeric locale's language, e.g. "de_DE.UTF-8"
    let locale = ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())?;
    let lang = locale
        .split(['_', '.', '@', '-'])
        .next()?
        .to_ascii_lowercase();
    const POINT: &[&str] = &[
        "c", "posix", "en", "zh", "ja", "ko", "he", "th", "hi", "ms", "fil",
    ];
    Some(if POINT.contains(&lang.as_str()) {
        '.'
    } else {
        ','
    })
}

/// Numbers shown with a decimal comma (Preferences > Decimal Separator).
static DECIMAL_COMMA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_decimal_comma(on: bool) {
    DECIMAL_COMMA.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn decimal_comma() -> bool {
    DECIMAL_COMMA.load(std::sync::atomic::Ordering::Relaxed)
}

/// A number's text with the chosen decimal separator; other text unchanged.
/// What is typed back is read by [`eui_core::num`], which takes either.
pub fn show_num(s: &str) -> String {
    if decimal_comma() {
        eui_core::num::with_comma(s)
    } else {
        s.to_string()
    }
}

/// A report value with the chosen number of decimal places.
pub fn fmt_fixed(v: f64) -> String {
    let n = DECIMALS.load(std::sync::atomic::Ordering::Relaxed);
    show_num(&format!("{v:.n$}"))
}

pub fn fmt_value(v: f32) -> String {
    show_num(&fmt_value_point(v))
}

fn fmt_value_point(v: f32) -> String {
    let a = v.abs();
    if !v.is_finite() {
        "—".into()
    } else if a == 0.0 {
        "0".into()
    } else if a >= 1000.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.2}")
    } else if a >= 0.1 {
        format!("{v:.3}")
    } else {
        format!("{v:.4}")
    }
}

/// Legend interval label as EPANET-UI shows it (shortest exact form).
pub fn fmt_break(v: f32) -> String {
    let s = format!("{v}");
    if s.len() > 8 {
        fmt_value(v)
    } else {
        show_num(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn reads_the_regional_decimal_separator() {
        assert!(matches!(system_decimal_separator(), Some('.' | ',')));
    }

    #[test]
    fn colors_follow_breaks() {
        let l = Legend::new(NodeTheme::Pressure.default_breaks());
        assert_eq!(l.color(10.0), RAMP[0]);
        assert_eq!(l.color(60.0), RAMP[2]);
        assert_eq!(l.color(150.0), RAMP[4]);
        assert_eq!(l.color(-60.0), RAMP[2], "flows are colored by magnitude");
    }

    #[test]
    fn interval_scale_matches_epanet_ui() {
        // EPANET-UI: AutoScale(109, 127) gives a step of 2 from 108.
        assert_eq!(interval_scale(109.0, 127.0), [110.0, 112.0, 114.0, 116.0]);
        assert_eq!(interval_scale(0.0, 0.0), [0.0, 1.0, 2.0, 3.0]);
        let b = interval_scale(0.012, 2.3);
        assert!(b.windows(2).all(|w| w[1] > w[0]), "{b:?}");
    }
}
